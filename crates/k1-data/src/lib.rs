//! «Князь: Легенды Лесной страны» (1999): работа с данными игры.
//!
//! Слои: игра на диске → репозиторий ([repo]) → формат ([map], [game_state],
//! [game_edit]).
//! Эталон — Python-пакет `konung/`: при расхождении прав Python (AGENTS.md).

pub mod game_edit;
pub mod game_state;
pub mod map;
pub mod reader;
pub mod repo;
pub mod writer;
