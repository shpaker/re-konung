//! Render-thread acknowledgement for deterministic control completion.
use bevy::{
    diagnostic::FrameCount,
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        render_resource::{CachedPipelineState, PipelineCache},
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[derive(Resource, Clone, Default)]
pub(crate) struct RenderReadiness(Arc<AtomicU64>);
#[derive(Resource, Default)]
struct ExtractedFrame(u32);

impl RenderReadiness {
    pub(crate) fn observed(&self) -> (u32, bool, bool) {
        let value = self.0.load(Ordering::Acquire);
        ((value >> 2) as u32, value & 1 != 0, value & 2 != 0)
    }
}

pub(crate) fn install(app: &mut App) {
    let readiness = RenderReadiness::default();
    app.insert_resource(readiness.clone());
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .insert_resource(readiness)
            .init_resource::<ExtractedFrame>()
            .add_systems(ExtractSchedule, extract_frame)
            .add_systems(Render, acknowledge.in_set(RenderSystems::Cleanup));
    }
}
fn extract_frame(frame: Extract<Res<FrameCount>>, mut extracted: ResMut<ExtractedFrame>) {
    extracted.0 = frame.0;
}
fn acknowledge(
    frame: Res<ExtractedFrame>,
    readiness: Res<RenderReadiness>,
    cache: Res<PipelineCache>,
) {
    let ready = cache.waiting_pipelines().next().is_none()
        && cache
            .pipelines()
            .any(|pipeline| matches!(pipeline.state, CachedPipelineState::Ok(_)))
        && !cache
            .pipelines()
            .any(|pipeline| matches!(pipeline.state, CachedPipelineState::Err(_)));
    let failed = cache
        .pipelines()
        .any(|pipeline| matches!(pipeline.state, CachedPipelineState::Err(_)));
    readiness.0.store(
        (u64::from(frame.0) << 2) | u64::from(ready) | (u64::from(failed) << 1),
        Ordering::Release,
    );
}
