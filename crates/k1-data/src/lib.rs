//! «Князь: Легенды Лесной страны» (1999): работа с данными игры.
//!
//! Слои: игра на диске → репозиторий ([repo]) → формат ([map], [game_state],
//! [game_edit], [graph_res], [objects_res], [lights_res], [flames_res],
//! [executable], [characters]).
//! Эталон — Python-пакет `konung/`: при расхождении прав Python (AGENTS.md).

pub mod characters;
pub mod executable;
pub mod flames_res;
pub mod game_edit;
pub mod game_state;
pub mod graph_res;
pub mod lights_res;
pub mod map;
pub mod objects_res;
pub mod palette;
pub mod reader;
pub mod repo;
pub mod rle;
pub mod writer;
