//! Numeric trace for `tools/audit_k1_lighting.py`; no game resources are emitted.
use k1_game::rules::time_of_day::{CYCLE_TICKS, NightPalette, ambient_value_with, ground_window};

fn main() {
    if std::env::args().nth(1).as_deref() == Some("maps") {
        for map in 0..56 {
            match k1_game::presentation::lighting::constant_map_color(map) {
                Some([r, g, b]) => println!("{r},{g},{b},1"),
                None => println!("0,0,0,0"),
            }
        }
        return;
    }
    for night in [NightPalette::Deep, NightPalette::Green] {
        for tick in 0..CYCLE_TICKS {
            let (r, g, b) = ambient_value_with(tick, night);
            println!(
                "{},{},{},{}",
                r as i8,
                g as i8,
                b as i8,
                u8::from(ground_window(tick))
            );
        }
    }
}
