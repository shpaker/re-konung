//! Специфика «Князя 1»: ресурсы игры, текстуры и правила отображения карты.
//!
//! Слой между форматами [k1_data] и общим фасадом `kn-game`: всё, что знает
//! про изометрию K1, палитры GRAPH.RES и порядок отрисовки из эталона
//! `konung/k1/render.py`, живёт здесь. Bevy не используется — логика
//! проверяется headless.

pub mod creature_anim;
pub mod creatures;
pub mod geometry;
pub mod resources;
pub mod rules;

pub mod presentation;
pub mod simulation;
