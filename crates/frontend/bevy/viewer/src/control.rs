//! Optional authenticated loopback control; mutations execute on the app thread.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::action::{ViewerAction, apply_action, state_value};
use crate::{Stats, Viewer, playback_for};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: Value,
    token: String,
    action: ViewerAction,
}

struct Client {
    stream: TcpStream,
    input: Vec<u8>,
    output: Vec<u8>,
    done: bool,
}
struct Pending {
    client: usize,
    id: Value,
    result: Result<Value, String>,
}
struct Capture {
    client: usize,
    id: Value,
    path: PathBuf,
    state: Value,
    playing: bool,
    started: bool,
    generation: u64,
}

#[derive(Resource)]
pub(crate) struct ControlServer {
    listener: TcpListener,
    path: PathBuf,
    token: String,
    clients: Vec<Client>,
    pending: Vec<Pending>,
    picks: Vec<(usize, Value, ViewerAction)>,
    capture: Option<Capture>,
    issued_frame: u32,
    issued_at: Option<std::time::Instant>,
    generation: u64,
    map_change: bool,
    requested_tick: Option<u64>,
}

#[derive(Resource, Default)]
struct WindowObservations(std::collections::HashMap<Entity, bool>);

pub(crate) fn window_occluded(world: &World, window: Entity) -> Option<bool> {
    world
        .get_resource::<WindowObservations>()
        .and_then(|observations| observations.0.get(&window).copied())
}

fn observe_windows(
    mut events: MessageReader<bevy::window::WindowOccluded>,
    mut observations: ResMut<WindowObservations>,
) {
    for event in events.read() {
        observations.0.insert(event.window, event.occluded);
    }
}

fn save_workspace_capture(image: &Image, path: &Path, snapshot: &Value) -> Result<Value, String> {
    let image = image
        .clone()
        .try_into_dynamic()
        .map_err(|error| error.to_string())?;
    let black = if let Some(rgba) = image.as_rgba8() {
        rgba.pixels()
            .all(|pixel| pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0)
    } else {
        image
            .to_rgb8()
            .pixels()
            .all(|pixel| pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0)
    };
    if image.width() == 0 || image.height() == 0 || black {
        return Err(
            "Window readback is empty or entirely black; no capture files were written".into(),
        );
    }
    image.save(path).map_err(|error| error.to_string())?;
    let metadata = path.with_extension("json");
    std::fs::write(
        &metadata,
        serde_json::to_vec_pretty(snapshot).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(json!({"image":path,"metadata":metadata}))
}

impl ControlServer {
    pub(crate) fn capturing(&self) -> bool {
        self.capture.is_some()
    }
    pub(crate) fn busy(&self) -> bool {
        self.capture.is_some() || !self.pending.is_empty() || !self.picks.is_empty()
    }
    pub(crate) fn mutating(&self) -> bool {
        self.busy()
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        let owned = std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|value| value["token"] == self.token);
        if owned {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub(crate) fn install(app: &mut App, path: &Path) -> Result<(), String> {
    let listener =
        TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| error.to_string())?;
    let token = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let session = json!({"version":1,"address":listener.local_addr().map_err(|error|error.to_string())?.to_string(),"token":token});
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    file.write_all(session.to_string().as_bytes())
        .map_err(|error| error.to_string())?;
    app.init_resource::<WindowObservations>()
        .add_message::<bevy::window::WindowOccluded>()
        .add_systems(Update, observe_windows.before(poll));
    app.insert_resource(ControlServer {
        listener,
        path: path.to_owned(),
        token,
        clients: Vec::new(),
        pending: Vec::new(),
        picks: Vec::new(),
        capture: None,
        issued_frame: 0,
        issued_at: None,
        generation: 0,
        map_change: false,
        requested_tick: None,
    })
    .add_systems(
        Update,
        poll.after(crate::action::drain_actions)
            .before(crate::apply_playback),
    )
    .add_systems(
        PostUpdate,
        finish
            .after(bevy::transform::TransformSystems::Propagate)
            .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
            .after(crate::input::pick),
    );
    Ok(())
}

fn error_code(error: &str) -> &'static str {
    if error.contains("timed out") {
        "request_timeout"
    } else if error.starts_with("Render") {
        "render_failed"
    } else if error.starts_with("Simulation") {
        "simulation_failed"
    } else if error.starts_with("Capture") {
        "capture_failed"
    } else if error.contains("token") {
        "unauthorized"
    } else if error.to_lowercase().contains("request") || error.contains("JSON") {
        "invalid_request"
    } else {
        "action_failed"
    }
}

fn response(id: Value, result: Result<Value, String>, state: Value) -> Vec<u8> {
    let mut value = match result { Ok(data) => json!({"id":id,"success":true,"data":data,"state":state}), Err(error) => json!({"id":id,"success":false,"error":{"code":error_code(&error),"message":error},"state":state}) }.to_string().into_bytes();
    value.push(b'\n');
    value
}

fn decode(line: &[u8], token: &str) -> Result<Request, (Value, String)> {
    let value: Value = serde_json::from_slice(line)
        .map_err(|error| (Value::Null, format!("Invalid request: {error}")))?;
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    if !id.is_string() && !id.is_number() {
        return Err((id, "A string or numeric request id is required".into()));
    }
    let request: Request = serde_json::from_value(value)
        .map_err(|error| (id.clone(), format!("Invalid request: {error}")))?;
    if request.token != token {
        return Err((id, "Invalid session token".into()));
    }
    Ok(request)
}

fn request_line(client: &mut Client) -> Result<Option<Vec<u8>>, String> {
    if !client.input.contains(&b'\n') {
        let mut buffer = [0u8; 4096];
        match client.stream.read(&mut buffer) {
            Ok(0) => {
                client.done = true;
                return Ok(None);
            }
            Ok(count) => client.input.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
            Err(_) => {
                client.done = true;
                return Ok(None);
            }
        }
    }
    if client.input.len() > 65536 {
        return Err("Request exceeds 64 KiB".into());
    }
    Ok(client
        .input
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|end| client.input[..end].to_vec()))
}

fn read_data(action: ViewerAction, state: &Value) -> Value {
    match action {
        ViewerAction::Creatures { search } => {
            let query = search.to_lowercase();
            Value::Array(
                state["creatures"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|creature| {
                        creature["label"]
                            .as_str()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&query)
                            || creature["id"].to_string().contains(&query)
                    })
                    .cloned()
                    .collect(),
            )
        }
        _ => Value::Null,
    }
}

fn poll(world: &mut World) {
    world.resource_scope(|world, mut server: Mut<ControlServer>| {
        for _ in 0..8 {
            match server.listener.accept() {
                Ok((stream,_))=>{
                    if stream.set_nonblocking(true).is_err() {continue;}
                    let free=server.clients.iter().enumerate().find_map(|(index,client)| {
                        (client.done && client.output.is_empty() && !server.pending.iter().any(|pending|pending.client==index) && !server.picks.iter().any(|pick|pick.0==index) && server.capture.as_ref().is_none_or(|capture|capture.client!=index)).then_some(index)
                    });
                    let client=Client {stream,input:Vec::new(),output:Vec::new(),done:false};
                    if let Some(index)=free {server.clients[index]=client;} else if server.clients.len()<64 {server.clients.push(client);}
                },
                Err(error) if error.kind()==std::io::ErrorKind::WouldBlock=>break,
                Err(_)=>break,
            }
        }
        if let Some(capture)=server.capture.as_ref() {
            world.resource_mut::<Viewer>().playback=playback_for(false);
            if let Some(tick)=capture.state["tick"].as_u64() {world.resource_mut::<crate::Clock>().ticks=tick;}
        }
        for index in 0..server.clients.len() {
            if server.clients[index].done {continue;}
            let line=match request_line(&mut server.clients[index]) {
                Ok(Some(line))=>line,
                Ok(None)=>continue,
                Err(error)=>{
                    server.clients[index].done=true;
                    server.clients[index].output=response(Value::Null,Err(error),Value::Null);
                    continue;
                },
            };
            let request=match decode(&line,&server.token) {
                Ok(request)=>request,
                Err((id,error))=>{
                    server.clients[index].done=true;
                    server.clients[index].output=response(id,Err(error),Value::Null);
                    continue;
                },
            };
            if matches!(request.action,ViewerAction::State {} | ViewerAction::Creatures {..}) {
                let state=state_value(world);
                let data=read_data(request.action,&state);
                server.clients[index].done=true;
                server.clients[index].output=response(request.id,Ok(data),state);
                continue;
            }
            if server.busy() {continue;}
            server.clients[index].done=true;
            server.generation=server.generation.wrapping_add(1);
            server.issued_at=Some(std::time::Instant::now());
            server.issued_frame=world.get_resource::<bevy::diagnostic::FrameCount>().map_or(0,|frame|frame.0);
            server.requested_tick=None;
            server.map_change = matches!(
                request.action,
                ViewerAction::Map { .. } | ViewerAction::RetryMap {}
            );
            if world.get_resource::<crate::ScrubState>().is_some_and(|scrub|scrub.active) {
                server.pending.push(Pending {client:index,id:request.id,result:Err("Finish or cancel the active timeline scrub before issuing a control action".into())});
                break;
            }
            if let ViewerAction::Capture { path } = request.action {
                if world.resource::<Stats>().map.is_none() {
                    server.pending.push(Pending {
                        client: index,
                        id: request.id,
                        result: Err("No map is displayed".into()),
                    });
                    break;
                }
                let playing = crate::playback_checked(world.resource::<Viewer>());
                world.resource_mut::<Viewer>().playback = playback_for(false);
                server.capture = Some(Capture {
                    client: index,
                    id: request.id,
                    path,
                    state: Value::Null,
                    playing,
                    started: false,
                    generation: server.generation,
                });
                break;
            }
            if matches!(request.action, ViewerAction::Pick { .. }) {
                server.picks.push((index, request.id, request.action));
                break;
            }
            let tick_action = matches!(
                request.action,
                ViewerAction::Seek { .. } | ViewerAction::Step { .. }
            );
            server.requested_tick = None;
            let result=apply_action(world,request.action).map(|()|Value::Null);
            if tick_action && result.is_ok() {
                server.requested_tick = Some(world.resource::<crate::Clock>().ticks);
            }
            server.pending.push(Pending {
                client: index,
                id: request.id,
                result,
            });
            break;
        }
    });
}

pub(crate) fn finish(world: &mut World) {
    let picks = std::mem::take(&mut world.resource_mut::<ControlServer>().picks);
    for (client, id, action) in picks {
        let result = apply_action(world, action).map(|()| Value::Null);
        world
            .resource_mut::<ControlServer>()
            .pending
            .push(Pending { client, id, result });
    }
    let mut state = state_value(world);
    world.resource_scope(|world, mut server: Mut<ControlServer>| {
        let failure = if server.requested_tick.is_some() && state["simulation_error"].is_string() {
            Some(format!(
                "Simulation failed: {}",
                state["simulation_error"].as_str().unwrap_or_default()
            ))
        } else if state["render_failed"] == true {
            Some("Render pipeline compilation failed; see the viewer log".to_owned())
        } else if server
            .issued_at
            .is_some_and(|issued| issued.elapsed() > std::time::Duration::from_secs(30))
        {
            Some("Capture/control request timed out while waiting for scene readiness".to_owned())
        } else {
            None
        };
        if let Some(error) = failure.as_ref() {
            if server.requested_tick.is_some() {
                let tick = world.resource::<crate::CreatureSim>().epoch
                    + world.resource::<crate::CreatureSim>().sim_ticks;
                world.resource_mut::<crate::Clock>().ticks = tick;
                world.resource_mut::<crate::Clock>().sub_tick = 0.0;
                world.resource_mut::<Viewer>().playback = playback_for(false);
                state = state_value(world);
            }
            for pending in &mut server.pending {
                pending.result = Err(error.clone());
            }
            if let Some(capture) = server.capture.take() {
                world.resource_mut::<Viewer>().playback = playback_for(capture.playing);
                server.clients[capture.client].output =
                    response(capture.id, Err(error.clone()), state.clone());
            }
        }
        if server
            .requested_tick
            .is_some_and(|tick| Some(tick) != state["tick"].as_u64())
            && failure.is_none()
        {
            for pending in &mut server.pending {
                pending.result = Err("Request canceled because the clock target changed".into());
            }
        }
        let rendered =
            state["render_frame"].as_u64().unwrap_or(0) >= u64::from(server.issued_frame);
        let settled = (state["ready"] == true
            || ((state["error"].is_string() || state["simulation_error"].is_string())
                && !state["displayed_map"].is_null()
                && state["render_ready"] == true))
            && rendered;
        if let Some(capture) = server.capture.as_mut()
            && !capture.started
            && settled
        {
            capture.started = true;
            capture.state = state.clone();
            let path = capture.path.clone();
            let snapshot = capture.state.clone();
            let generation = capture.generation;
            world.spawn(Screenshot::primary_window()).observe(
                move |event: On<ScreenshotCaptured>,
                      mut server: ResMut<ControlServer>,
                      mut viewer: ResMut<Viewer>| {
                    if server
                        .capture
                        .as_ref()
                        .is_none_or(|capture| capture.generation != generation)
                    {
                        return;
                    }
                    let result = save_workspace_capture(&event.image, &path, &snapshot)
                        .map_err(|error| format!("Capture failed: {error}"));
                    if let Some(capture) = server.capture.take() {
                        viewer.playback = playback_for(capture.playing);
                        let output = response(capture.id, result, capture.state);
                        server.clients[capture.client].output = output;
                    }
                },
            );
        }
        if !server.pending.is_empty()
            && (settled || state["error"].is_string() || server.pending[0].result.is_err())
        {
            let pending = std::mem::take(&mut server.pending);
            for mut pending in pending {
                if pending.result.is_ok()
                    && server.map_change
                    && let Some(error) = state["error"].as_str()
                {
                    pending.result = Err(error.into());
                }
                server.clients[pending.client].output =
                    response(pending.id, pending.result, state.clone());
            }
        }
        for client in &mut server.clients {
            if client.output.is_empty() {
                continue;
            }
            match client.stream.write(&client.output) {
                Ok(count) => {
                    client.output.drain(..count);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => client.output.clear(),
            }
        }
        if server.capture.is_none() && server.pending.is_empty() && server.picks.is_empty() {
            server
                .clients
                .retain(|client| !client.done || !client.output.is_empty());
            if server.pending.is_empty() && server.picks.is_empty() {
                server.issued_at = None;
                server.requested_tick = None;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requests_require_valid_envelope_and_token() {
        for input in [
            "oops",
            "{}",
            r#"{"token":"secret","action":{"type":"state"}}"#,
            r#"{"id":1,"token":"wrong","action":{"type":"state"}}"#,
            r#"{"id":1,"token":"secret","action":{"type":"select"}}"#,
        ] {
            assert!(decode(input.as_bytes(), "secret").is_err(), "{input}");
        }
        assert!(
            decode(
                br#"{"id":"a","token":"secret","action":{"type":"state"}}"#,
                "secret"
            )
            .is_ok()
        );
    }
    fn app() -> (App, tempfile::TempDir, String) {
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::Viewer {
                maps: vec![kn_game::MapRef {
                    source: kn_game::MapSource::Game,
                    number: 1,
                }],
                selected: 0,
                overlays: crate::DEFAULT_OVERLAYS,
                playback: crate::PlayState::Playing,
                constant_day: false,
            })
            .insert_resource(crate::Stats {
                map: Some(kn_game::MapRef {
                    source: kn_game::MapSource::Game,
                    number: 1,
                }),
                ..default()
            })
            .insert_resource(crate::Clock::new(60.0))
            .init_resource::<crate::CreatureSim>()
            .init_resource::<crate::CameraState>()
            .init_resource::<crate::Rebuild>()
            .init_resource::<crate::action::ViewerWorkspace>();
        install(&mut app, &directory.path().join("session.json")).unwrap();
        let token = app.world().resource::<ControlServer>().token.clone();
        (app, directory, token)
    }
    fn request(app: &App, token: &str, id: u32, action: Value) -> TcpStream {
        let mut stream = TcpStream::connect(
            app.world()
                .resource::<ControlServer>()
                .listener
                .local_addr()
                .unwrap(),
        )
        .unwrap();
        writeln!(stream, "{}", json!({"id":id,"token":token,"action":action})).unwrap();
        stream.set_nonblocking(true).unwrap();
        stream
    }
    fn received(stream: &mut TcpStream) -> Option<Value> {
        let mut bytes = vec![0; 65536];
        match stream.read(&mut bytes) {
            Ok(count) if count > 0 => Some(serde_json::from_slice(&bytes[..count]).unwrap()),
            _ => None,
        }
    }
    fn drive_until(app: &mut App, condition: impl Fn(&mut World) -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !condition(app.world_mut()) {
            assert!(
                std::time::Instant::now() < deadline,
                "viewer did not reach the expected test state"
            );
            app.update();
            std::thread::yield_now();
        }
    }
    fn receive_response(app: &mut App, stream: &mut TcpStream) -> Value {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut bytes = Vec::new();
        loop {
            let mut buffer = [0; 4096];
            match stream.read(&mut buffer) {
                Ok(count) if count > 0 => bytes.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                other => panic!("unexpected response read {other:?}"),
            }
            if bytes.contains(&b'\n') {
                return serde_json::from_slice(&bytes).unwrap();
            }
            assert!(
                std::time::Instant::now() < deadline,
                "viewer did not complete the test request"
            );
            app.update();
            std::thread::yield_now();
        }
    }
    #[test]
    fn pending_readiness_keeps_client_and_serializes_later_actions() {
        let (mut app, _directory, token) = app();
        app.world_mut().resource_mut::<crate::Rebuild>().map = true;
        let mut first = request(
            &app,
            &token,
            1,
            json!({"type":"display","layer":"lights","visible":true}),
        );
        let mut second = request(&app, &token, 2, json!({"type":"playback","playing":false}));
        drive_until(&mut app, |world| {
            world.resource::<ControlServer>().pending.len() == 1
                && world.resource::<ControlServer>().clients.len() == 2
        });
        assert!(received(&mut first).is_none());
        assert_eq!(app.world().resource::<ControlServer>().pending.len(), 1);
        assert_eq!(app.world().resource::<ControlServer>().clients.len(), 2);
        assert_eq!(
            app.world().resource::<Viewer>().playback,
            crate::PlayState::Playing
        );
        app.world_mut().resource_mut::<crate::Rebuild>().map = false;
        let first = receive_response(&mut app, &mut first);
        assert_eq!(first["id"], 1);
        assert_eq!(first["state"]["playing"], true);
        let second = receive_response(&mut app, &mut second);
        assert_eq!(second["id"], 2);
        assert_eq!(second["state"]["playing"], false);
    }
    #[test]
    fn capture_responds_after_both_saves_and_restores_playback() {
        let (mut app, directory, token) = app();
        let path = directory.path().join("capture.png");
        let mut stream = request(&app, &token, 3, json!({"type":"capture","path":path}));
        drive_until(&mut app, |world| {
            world
                .query_filtered::<Entity, With<Screenshot>>()
                .iter(world)
                .next()
                .is_some()
        });
        assert!(app.world().resource::<ControlServer>().capturing());
        assert_eq!(
            app.world().resource::<Viewer>().playback,
            crate::PlayState::Paused
        );
        assert!(received(&mut stream).is_none());
        assert!(!path.exists());
        let entity = app
            .world_mut()
            .query_filtered::<Entity, With<Screenshot>>()
            .single(app.world())
            .unwrap();
        let image = Image::new_fill(
            bevy::render::render_resource::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            &[30, 20, 10, 255],
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::MAIN_WORLD,
        );
        app.world_mut()
            .trigger(ScreenshotCaptured { entity, image });
        assert!(path.exists() && path.with_extension("json").exists());
        assert_eq!(
            app.world().resource::<Viewer>().playback,
            crate::PlayState::Playing
        );
        let response = receive_response(&mut app, &mut stream);
        assert_eq!(response["success"], true);
        let snapshot: Value =
            serde_json::from_slice(&std::fs::read(path.with_extension("json")).unwrap()).unwrap();
        assert_eq!(response["state"], snapshot);
    }
    #[test]
    fn an_expired_capture_callback_cannot_consume_a_later_capture() {
        let (mut app, directory, token) = app();
        let mut first = request(
            &app,
            &token,
            4,
            json!({"type":"capture","path":directory.path().join("expired.png")}),
        );
        drive_until(&mut app, |world| {
            world
                .query_filtered::<Entity, With<Screenshot>>()
                .iter(world)
                .next()
                .is_some()
        });
        let old_entity = app
            .world_mut()
            .query_filtered::<Entity, With<Screenshot>>()
            .single(app.world())
            .unwrap();
        app.world_mut().resource_mut::<ControlServer>().issued_at =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(31));
        assert_eq!(
            receive_response(&mut app, &mut first)["error"]["code"],
            "request_timeout"
        );
        let _second = request(
            &app,
            &token,
            5,
            json!({"type":"capture","path":directory.path().join("current.png")}),
        );
        drive_until(&mut app, |world| {
            world.resource::<ControlServer>().capture.is_some()
        });
        let generation = app
            .world()
            .resource::<ControlServer>()
            .capture
            .as_ref()
            .unwrap()
            .generation;
        app.world_mut().trigger(ScreenshotCaptured {
            entity: old_entity,
            image: Image::default(),
        });
        assert_eq!(
            app.world()
                .resource::<ControlServer>()
                .capture
                .as_ref()
                .unwrap()
                .generation,
            generation
        );
        assert!(!directory.path().join("expired.png").exists());
    }
    #[test]
    fn timed_out_seek_stops_at_completed_wrapper_progress() {
        let (mut app, _directory, token) = app();
        app.world_mut().resource_mut::<crate::Rebuild>().map = true;
        let mut stream = request(&app, &token, 6, json!({"type":"seek","tick":1000000}));
        drive_until(&mut app, |world| {
            world.resource::<ControlServer>().requested_tick.is_some()
        });
        app.world_mut().resource_mut::<crate::CreatureSim>().epoch = 12;
        app.world_mut()
            .resource_mut::<crate::CreatureSim>()
            .sim_ticks = 34;
        app.world_mut().resource_mut::<ControlServer>().issued_at =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(31));
        let response = receive_response(&mut app, &mut stream);
        assert_eq!(response["error"]["code"], "request_timeout");
        assert_eq!(response["state"]["tick"], 46);
        assert_eq!(app.world().resource::<crate::Clock>().ticks, 46);
        assert_eq!(
            app.world().resource::<Viewer>().playback,
            crate::PlayState::Paused
        );
    }
    #[test]
    fn replay_failure_is_reported_promptly_and_state_remains_readable() {
        let (mut app, _directory, token) = app();
        app.world_mut().resource_mut::<crate::Rebuild>().map = true;
        let mut stream = request(&app, &token, 7, json!({"type":"seek","tick":1000000}));
        drive_until(&mut app, |world| {
            world.resource::<ControlServer>().requested_tick.is_some()
        });
        app.world_mut().resource_mut::<crate::CreatureSim>().error =
            Some("synthetic replay failure".into());
        let response = receive_response(&mut app, &mut stream);
        assert_eq!(response["error"]["code"], "simulation_failed");
        assert_eq!(response["state"]["tick"], 0);
        let mut state = request(&app, &token, 8, json!({"type":"state"}));
        assert_eq!(receive_response(&mut app, &mut state)["success"], true);
    }
    #[test]
    fn queued_ui_seek_and_reset_do_not_overtake_an_active_control_seek() {
        let (mut app, _directory, token) = app();
        app.init_resource::<crate::action::ActionQueue>()
            .add_systems(Update, crate::action::drain_actions);
        app.world_mut().resource_mut::<crate::Rebuild>().map = true;
        let mut stream = request(&app, &token, 9, json!({"type":"seek","tick":1000000}));
        drive_until(&mut app, |world| {
            world.resource::<ControlServer>().requested_tick.is_some()
        });
        app.world_mut()
            .resource_mut::<crate::action::ActionQueue>()
            .0
            .push_back(ViewerAction::Seek { tick: 10 });
        app.world_mut()
            .resource_mut::<crate::action::ActionQueue>()
            .0
            .push_back(ViewerAction::ResetAnimation {});
        app.update();
        assert_eq!(app.world().resource::<crate::Clock>().ticks, 1000000);
        assert_eq!(
            app.world().resource::<crate::action::ActionQueue>().0.len(),
            2
        );
        app.world_mut().resource_mut::<crate::Rebuild>().map = false;
        let response = receive_response(&mut app, &mut stream);
        assert_eq!(response["success"], true);
        assert_eq!(response["state"]["tick"], 1000000);
        app.update();
        assert_eq!(app.world().resource::<crate::Clock>().ticks, 10);
        assert!(
            app.world()
                .resource::<crate::action::ActionQueue>()
                .0
                .is_empty()
        );
    }
    #[test]
    fn state_reads_return_a_snapshot_during_lighting_preview() {
        let (mut app, _directory, token) = app();
        app.world_mut().insert_resource(crate::ScrubState {
            active: true,
            day: 0,
            target: 900,
            ..default()
        });
        app.world_mut().resource_mut::<crate::Clock>().ticks = 300;
        let mut stream = request(&app, &token, 10, json!({"type":"state"}));
        let response = receive_response(&mut app, &mut stream);
        assert_eq!(response["success"], true);
        assert_eq!(response["state"]["phase"], "lighting_preview");
        assert_eq!(response["state"]["preview_tick"], 900);
        assert_eq!(response["state"]["tick"], 300);
        assert!(app.world().resource::<crate::ScrubState>().active);
    }
    #[test]
    fn progress_reads_during_a_pending_seek_preserve_active_bookkeeping_and_client_slots() {
        let (mut app, _directory, token) = app();
        app.world_mut().resource_mut::<crate::Rebuild>().map = true;
        let mut seeking = request(&app, &token, 11, json!({"type":"seek","tick":1000000}));
        drive_until(&mut app, |world| {
            world.resource::<ControlServer>().requested_tick.is_some()
        });
        let issued_at = app.world().resource::<ControlServer>().issued_at;
        let issued_frame = app.world().resource::<ControlServer>().issued_frame;
        let generation = app.world().resource::<ControlServer>().generation;
        for id in 20..100 {
            let mut reader = request(&app, &token, id, json!({"type":"state"}));
            let response = receive_response(&mut app, &mut reader);
            assert_eq!(response["success"], true);
            assert_eq!(response["state"]["tick"], 1000000);
            let server = app.world().resource::<ControlServer>();
            assert_eq!(server.issued_at, issued_at);
            assert_eq!(server.issued_frame, issued_frame);
            assert_eq!(server.generation, generation);
            assert_eq!(server.requested_tick, Some(1000000));
            assert_eq!(server.pending.len(), 1);
            assert!(server.clients.len() <= 3);
        }
        assert!(received(&mut seeking).is_none());
    }
    #[test]
    fn unauthenticated_requests_do_not_receive_workspace_state() {
        let (mut app, _directory, _token) = app();
        let mut stream = request(&app, "wrong", 101, json!({"type":"state"}));
        let response = receive_response(&mut app, &mut stream);
        assert_eq!(response["error"]["code"], "unauthorized");
        assert_eq!(response["state"], Value::Null);
        assert!(response.get("creatures").is_none());
        assert!(response.get("camera").is_none());
    }
    #[test]
    fn state_reads_are_available_while_capture_readback_is_pending() {
        let (mut app, directory, token) = app();
        let mut capturing = request(
            &app,
            &token,
            102,
            json!({"type":"capture","path":directory.path().join("pending.png")}),
        );
        drive_until(&mut app, |world| {
            world.resource::<ControlServer>().capturing()
        });
        let mut reader = request(&app, &token, 103, json!({"type":"state"}));
        assert_eq!(receive_response(&mut app, &mut reader)["success"], true);
        assert!(app.world().resource::<ControlServer>().capturing());
        assert!(received(&mut capturing).is_none());
    }
    #[test]
    fn black_readback_fails_without_writing_files_and_restores_playback() {
        let (mut app, directory, token) = app();
        let path = directory.path().join("black.png");
        let mut stream = request(&app, &token, 200, json!({"type":"capture","path":path}));
        drive_until(&mut app, |world| {
            world
                .query_filtered::<Entity, With<Screenshot>>()
                .iter(world)
                .next()
                .is_some()
        });
        let entity = app
            .world_mut()
            .query_filtered::<Entity, With<Screenshot>>()
            .single(app.world())
            .unwrap();
        let image = Image::new_fill(
            bevy::render::render_resource::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            &[0, 0, 0, 255],
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::MAIN_WORLD,
        );
        app.world_mut()
            .trigger(ScreenshotCaptured { entity, image });
        let response = receive_response(&mut app, &mut stream);
        assert_eq!(response["success"], false);
        assert_eq!(response["error"]["code"], "capture_failed");
        assert!(
            response["error"]["message"]
                .as_str()
                .unwrap()
                .contains("entirely black")
        );
        assert!(!path.exists() && !path.with_extension("json").exists());
        assert_eq!(
            app.world().resource::<Viewer>().playback,
            crate::PlayState::Playing
        );
        let empty = Image {
            data: None,
            ..default()
        };
        assert!(save_workspace_capture(&empty, &path, &Value::Null).is_err());
        assert!(!path.exists());
    }
    #[test]
    fn state_exposes_focus_visibility_and_observed_occlusion_without_guessing() {
        let (mut app, _directory, token) = app();
        let window = app
            .world_mut()
            .spawn(Window {
                focused: false,
                visible: true,
                ..default()
            })
            .id();
        assert_eq!(window_occluded(app.world(), window), None);
        app.world_mut().write_message(bevy::window::WindowOccluded {
            window,
            occluded: true,
        });
        let mut stream = request(&app, &token, 201, json!({"type":"state"}));
        let state = receive_response(&mut app, &mut stream)["state"].clone();
        assert_eq!(state["window"]["focused"], false);
        assert_eq!(state["window"]["visible"], true);
        assert_eq!(state["window"]["occluded"], true);
    }
}
