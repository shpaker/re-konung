//! Кадровая анимация спрайтов: часы, последовательность кадров и применение их
//! к [`Sprite`].
//!
//! Модуль не знает ни об игре, ни о форматах: он берёт готовые картинки и
//! пиксельные смещения, а откуда они взялись решает тот, кто их собрал.
//!
//! Длительность кадра — в **тиках**, а не в секундах: часы остаются целыми,
//! покадровый просмотр равен одному тику, а куски с одной фазой идут в такт без
//! отдельного механизма синхронизации. Реальное время в тики переводит
//! [`Clock::advance`], которое копит дробный остаток: без него на тике в 60 мс
//! при 120 гц терялась бы половина каждого кадра, и анимация шла бы медленнее.

use std::sync::Arc;

use bevy::prelude::*;

use crate::scene::piece_center;

/// Что происходит с воспроизведением.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayState {
    /// Часы идут.
    #[default]
    Playing,
    /// Часы стоят: остаётся тот кадр, на котором анимацию поставили на паузу.
    Paused,
    /// Анимации нет: часы обнулены, поэтому показан базовый кадр — тот, что
    /// записан в данных, — и с него же воспроизведение продолжится.
    Off,
}

/// Часы анимации: одни на всю сцену.
///
/// Одни часы — не упрощение, а семантика: в игре кадры меняются по общему тику,
/// поэтому одинаковые объекты и персонажи должны идти в такт (как и в
/// расширении Tiled, где часы анимации тоже одни на все тайлы).
#[derive(Resource, Debug)]
pub struct Clock {
    /// Сколько тиков прошло с обнуления.
    pub ticks: u64,
    /// Реального времени, не дотягивающего до следующего тика, секунд.
    pub sub_tick: f32,
    pub state: PlayState,
    /// Длительность тика, миллисекунд.
    tick_ms: f32,
}

impl Clock {
    /// Часы с нулевым тиком и заданной длительностью тика.
    pub fn new(tick_ms: f32) -> Self {
        Self {
            ticks: 0,
            sub_tick: 0.0,
            state: PlayState::default(),
            tick_ms,
        }
    }

    /// Пропускает через часы кадр длиной `delta_secs`; в [`PlayState::Off`] и
    /// на паузе время не идёт.
    pub fn advance(&mut self, delta_secs: f32) {
        if self.state != PlayState::Playing {
            return;
        }
        let tick_secs = self.tick_ms / 1000.0;
        if !positive(tick_secs) || !positive(delta_secs) {
            return;
        }
        let elapsed = self.sub_tick + delta_secs;
        let passed = (elapsed / tick_secs).floor() as u64;
        if passed == 0 {
            self.sub_tick = elapsed;
            return;
        }
        self.ticks += passed;
        self.sub_tick = elapsed - passed as f32 * tick_secs;
    }

    /// Шаг покадрового просмотра: ровно на `delta` тиков, назад не ниже нуля.
    ///
    /// Остаток дробного времени сбрасывается: шаг означает «показать соседний
    /// кадр целиком», а не «подвинуть часы на случайную долю кадра». В
    /// [`PlayState::Off`] шаг бесполезен — там показан базовый кадр, и часы
    /// должны остаться на нуле.
    pub fn step(&mut self, delta: i64) {
        if self.state == PlayState::Off {
            return;
        }
        self.ticks = (self.ticks as i64 + delta).max(0) as u64;
        self.sub_tick = 0.0;
    }

    /// Переключает воспроизведение; [`PlayState::Off`] обнуляет часы, чтобы
    /// показать базовый кадр и с него же продолжить.
    pub fn goto(&mut self, state: PlayState) {
        if state == PlayState::Off {
            self.ticks = 0;
            self.sub_tick = 0.0;
        }
        self.state = state;
    }
}

/// Длительность, с которой часы можно двигать: конечная и больше нуля.
fn positive(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

/// Один кадр последовательности.
#[derive(Debug, Clone)]
pub struct Frame {
    /// `None` — рисовать этот слой в этом кадре нечем: спрайт прячется, а кадр
    /// остаётся в последовательности, чтобы не сдвигать тайминг остальных.
    pub image: Option<Handle<Image>>,
    /// Размер картинки кадра в пикселях.
    pub size: Vec2,
    /// Смещение левого верхнего угла кадра от позиции куска, пиксели карты.
    pub offset: Vec2,
    /// Сколько тиков длится кадр.
    pub ticks: u32,
}

impl Frame {
    /// Кадр целиком: картинка данного размера в данном смещении, на один тик.
    pub fn new(image: Handle<Image>, size: Vec2, offset: Vec2) -> Self {
        Self {
            image: Some(image),
            size,
            offset,
            ticks: 1,
        }
    }
}

/// Циклическая последовательность кадров, общая для всех кусков одной анимации
/// ( [`Arc`] делится между слоями персонажа и между копиями одного объекта).
#[derive(Debug)]
pub struct Sequence {
    pub frames: Vec<Frame>,
    /// Сумма длительностей кадров: по ней часы заворачивают цикл.
    pub ticks: u64,
}

impl Sequence {
    pub fn new(frames: Vec<Frame>) -> Self {
        let ticks = frames.iter().map(|frame| u64::from(frame.ticks)).sum();
        Self { frames, ticks }
    }

    /// Кадр, который показывают часы `ticks` со сдвигом `phase`.
    pub fn frame_at(&self, ticks: u64, phase: u64) -> usize {
        if self.ticks == 0 {
            return 0;
        }
        let mut point = (ticks + phase) % self.ticks;
        for (index, frame) in self.frames.iter().enumerate() {
            let length = u64::from(frame.ticks);
            if point < length {
                return index;
            }
            point -= length;
        }
        self.frames.len() - 1
    }

    /// Сдвиг фазы, при котором нулевой тик часов показывает `base`-й кадр.
    ///
    /// Так базовый кадр — кадр из файла — остаётся видимым и до начала
    /// воспроизведения, и в [`PlayState::Off`]: сверка со статичным рендером не
    /// требует особого кода в системе применения.
    pub fn phase_for(&self, base: usize) -> u64 {
        if self.ticks == 0 {
            return 0;
        }
        self.frames
            .iter()
            .take(base)
            .map(|frame| u64::from(frame.ticks))
            .sum::<u64>()
            % self.ticks
    }
}

/// Позиция и размер спрайта для кадра.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteView {
    pub size: Vec2,
    pub center: Vec2,
}

/// Геометрия кадра: смещение кадра добавляется к углу куска, а центр считается
/// по размеру этого кадра.
///
/// Спрайт Bevy позиционируется по центру, а кадры одной анимации в «Князе»
/// разные по размеру — с центром по старому размеру картинка прыгала бы на
/// каждом кадре.
pub fn frame_view(x: i32, y: i32, frame: &Frame) -> SpriteView {
    let center = piece_center(
        x as f32 + frame.offset.x,
        y as f32 + frame.offset.y,
        frame.size.x,
        frame.size.y,
    );
    SpriteView {
        size: frame.size,
        center,
    }
}

/// Анимированный спрайт: какая последовательность, с какой фазой и где лежит
/// кусок.
#[derive(Component, Debug)]
pub struct Animator {
    pub sequence: Arc<Sequence>,
    /// Сдвиг в тиках: общий у всех слоёв одной анимации, поэтому они и синхронны.
    pub phase: u64,
    /// Левый верхний угол куска в пикселях карты, до смещения кадра.
    pub x: i32,
    pub y: i32,
    /// Кадр, показанный сейчас; пишет [apply_animators].
    shown: Option<usize>,
}

impl Animator {
    /// Аниматор с нулевой фазой: первый кадр последовательности.
    pub fn new(sequence: Arc<Sequence>, x: i32, y: i32) -> Self {
        Self {
            sequence,
            phase: 0,
            x,
            y,
            shown: None,
        }
    }

    /// Аниматор, который при нуле часов показывает `base`-й кадр.
    pub fn with_base(sequence: Arc<Sequence>, x: i32, y: i32, base: usize) -> Self {
        let phase = sequence.phase_for(base);
        Self {
            sequence,
            phase,
            x,
            y,
            shown: None,
        }
    }

    /// Кадр на экране сейчас.
    pub fn shown(&self) -> Option<usize> {
        self.shown
    }
}

/// Плагин кадровой анимации. `tick_ms` — длительность одного тика сцены.
pub struct AnimPlugin {
    tick_ms: f32,
}

impl AnimPlugin {
    pub fn new(tick_ms: f32) -> Self {
        Self { tick_ms }
    }
}

impl Plugin for AnimPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Clock::new(self.tick_ms))
            .add_systems(Update, (advance_clock, apply_animators).chain());
    }
}

fn advance_clock(time: Res<Time>, mut clock: ResMut<Clock>) {
    clock.advance(time.delta_secs());
}

/// Переводит спрайты на кадр часов.
///
/// Поля спрайта меняются только когда кадр сменился: запись тех же значений
/// каждый кадр помечала бы компоненты изменёнными, и рендер заново вытягивал бы
/// всю карту. Порядок наложения (z) не трогается — он задан один раз при сборке
/// сцены по базовому кадру.
fn apply_animators(
    clock: Res<Clock>,
    mut query: Query<(&mut Animator, &mut Sprite, &mut Transform, &mut Visibility)>,
) {
    for (mut animator, mut sprite, mut transform, mut visibility) in &mut query {
        let index = animator.sequence.frame_at(clock.ticks, animator.phase);
        if animator.shown == Some(index) {
            continue;
        }
        animator.shown = Some(index);
        let Some(frame) = animator.sequence.frames.get(index) else {
            continue;
        };
        let Some(image) = frame.image.clone() else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Visible;
        sprite.image = image;
        let view = frame_view(animator.x, animator.y, frame);
        sprite.custom_size = Some(view.size);
        transform.translation.x = view.center.x;
        transform.translation.y = view.center.y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(size: f32, ticks: u32) -> Frame {
        Frame {
            image: Some(Handle::default()),
            size: Vec2::splat(size),
            offset: Vec2::ZERO,
            ticks,
        }
    }

    fn sequence(sizes: &[f32], ticks: &[u32]) -> Sequence {
        Sequence::new(
            sizes
                .iter()
                .zip(ticks)
                .map(|(&size, &tick)| frame(size, tick))
                .collect(),
        )
    }

    #[test]
    fn clock_keeps_the_fraction_of_a_tick() {
        // 16.7 мс при 60-мс тике: четыре кадра — уже один тик, и остаток жив
        let mut clock = Clock::new(60.0);
        for _ in 0..4 {
            clock.advance(0.0167);
        }
        assert_eq!(clock.ticks, 1);
        assert!(clock.sub_tick > 0.0 && clock.sub_tick < 0.06);
    }

    #[test]
    fn a_second_of_frames_gives_the_whole_ticks() {
        // Потеря остатка сделала бы анимацию медленнее: 60 кадров по 1/60 с —
        // ровно 16 тиков по 60 мс, а не 15
        let mut clock = Clock::new(60.0);
        for _ in 0..60 {
            clock.advance(1.0 / 60.0);
        }
        assert_eq!(clock.ticks, 16);
    }

    #[test]
    fn long_frames_become_several_ticks() {
        let mut clock = Clock::new(60.0);
        clock.advance(0.13);
        assert_eq!(clock.ticks, 2);
    }

    #[test]
    fn a_broken_tick_leaves_the_clock_alone() {
        // Нулевой или бесконечный тик не должен ни двигать часы, ни портить остаток
        for tick_ms in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let mut clock = Clock::new(tick_ms);
            clock.advance(1.0);
            assert_eq!(
                (clock.ticks, clock.sub_tick),
                (0, 0.0),
                "tick_ms = {tick_ms}"
            );
        }
    }

    #[test]
    fn pause_freezes_ticks_and_off_resets_them() {
        let mut clock = Clock::new(60.0);
        clock.advance(0.13);
        clock.goto(PlayState::Paused);
        clock.advance(1.0);
        assert_eq!(clock.ticks, 2, "на паузе время не идёт");

        clock.goto(PlayState::Playing);
        clock.advance(0.06);
        assert_eq!(clock.ticks, 3);

        clock.goto(PlayState::Off);
        assert_eq!((clock.ticks, clock.sub_tick), (0, 0.0));
        clock.advance(1.0);
        assert_eq!(clock.ticks, 0, "выключенные часы стоят на базовом кадре");
    }

    #[test]
    fn step_is_exactly_one_tick_and_never_negative() {
        let mut clock = Clock::new(60.0);
        clock.advance(0.2);
        let ticks = clock.ticks;
        clock.step(1);
        assert_eq!(clock.ticks, ticks + 1);
        assert_eq!(clock.sub_tick, 0.0);
        clock.step(-100);
        assert_eq!(clock.ticks, 0);
    }

    #[test]
    fn step_does_not_move_a_hidden_animation() {
        let mut clock = Clock::new(60.0);
        clock.goto(PlayState::Off);
        clock.step(3);
        assert_eq!(clock.ticks, 0);
    }

    #[test]
    fn uniform_sequence_loops_over_its_length() {
        let sequence = sequence(&[10.0, 10.0, 10.0, 10.0], &[1, 1, 1, 1]);
        assert_eq!(sequence.ticks, 4);
        assert_eq!(sequence.frame_at(0, 0), 0);
        assert_eq!(sequence.frame_at(2, 0), 2);
        assert_eq!(sequence.frame_at(4, 0), 0, "цикл");
        assert_eq!(sequence.frame_at(7, 0), 3);
    }

    #[test]
    fn frame_durations_are_respected() {
        // Объект с frame_time = 3: кадр длится три тика
        let sequence = sequence(&[8.0, 8.0, 8.0], &[3, 1, 2]);
        assert_eq!(sequence.ticks, 6);
        assert_eq!(
            (0..sequence.ticks)
                .map(|tick| sequence.frame_at(tick, 0))
                .collect::<Vec<_>>(),
            [0, 0, 0, 1, 2, 2]
        );
    }

    #[test]
    fn phase_puts_the_base_frame_at_zero_ticks() {
        let sequence = sequence(&[8.0; 6], &[2, 2, 2, 2, 2, 2]);
        for base in 0..sequence.frames.len() {
            let phase = sequence.phase_for(base);
            assert_eq!(sequence.frame_at(0, phase), base);
            // И через полный цикл тот же кадр
            assert_eq!(sequence.frame_at(sequence.ticks, phase), base);
        }
    }

    #[test]
    fn empty_sequence_is_harmless() {
        let sequence = Sequence::new(Vec::new());
        assert_eq!(sequence.ticks, 0);
        assert_eq!(sequence.frame_at(5, 0), 0);
        assert_eq!(sequence.phase_for(3), 0);
    }

    #[test]
    fn center_follows_the_frame_size() {
        // Кадры 40×16 и 20×20 в одном углу: центр обязан переехать, иначе
        // картинка прыгает
        let mut big = frame(0.0, 1);
        big.size = Vec2::new(40.0, 16.0);
        let big = frame_view(10, 100, &big);
        let small = frame_view(10, 100, &frame(20.0, 1));
        assert_eq!(big.center, Vec2::new(30.0, -108.0));
        assert_eq!(big.size, Vec2::new(40.0, 16.0));
        assert_eq!(small.center, Vec2::new(20.0, -110.0));
        assert_eq!(small.size, Vec2::splat(20.0));
    }

    #[test]
    fn frame_offset_moves_the_center_and_keeps_the_axis_flip() {
        // Кадр 8×8 со смещением (−3, −5) от угла куска (0, 0): левый верхний
        // угол в (−3, −5), центр в мире Bevy — (1, 1): y развёрнут
        let mut frame = frame(8.0, 1);
        frame.offset = Vec2::new(-3.0, -5.0);
        let view = frame_view(0, 0, &frame);
        assert_eq!(view.center, Vec2::new(1.0, 1.0));
    }

    #[test]
    fn hidden_frames_stay_in_the_sequence() {
        // Слой без картинки в одном кадре не сокращает цикл: тайминг остальных
        // кадров не сдвигается
        let mut gap = frame(8.0, 1);
        gap.image = None;
        let sequence = Sequence::new(vec![frame(8.0, 1), gap, frame(8.0, 1)]);
        assert_eq!(sequence.ticks, 3);
        assert_eq!(sequence.frame_at(1, 0), 1);
        assert!(sequence.frames[1].image.is_none());
    }

    #[test]
    fn animator_starts_on_the_base_frame() {
        let sequence = Arc::new(sequence(&[8.0; 4], &[1, 1, 1, 1]));
        let animator = Animator::with_base(Arc::clone(&sequence), 0, 0, 2);
        assert_eq!(sequence.frame_at(0, animator.phase), 2);
        assert_eq!(animator.shown(), None);
        assert_eq!(Animator::new(sequence, 0, 0).phase, 0);
    }

    /// Живой мир Bevy без окна и рендерера: часы → что лежит на спрайте.
    /// Возвращает приложение и сущность единственного спрайта.
    fn scene(frames: Vec<Frame>, base: usize) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            // Только применение: advance_clock берёт реальное время кадра,
            // тестовые часы двигает сам тест
            .add_systems(Update, apply_animators)
            .insert_resource(Clock::new(60.0));
        let entity = app
            .world_mut()
            .spawn((
                Sprite::default(),
                Animator::with_base(Arc::new(Sequence::new(frames)), 100, 200, base),
            ))
            .id();
        // Пауза: кадры двигает только то, что поставит тест
        app.world_mut()
            .resource_mut::<Clock>()
            .goto(PlayState::Paused);
        (app, entity)
    }

    #[test]
    fn clock_moves_the_sprite() {
        let (mut app, entity) = scene(vec![frame(10.0, 1), frame(20.0, 1), frame(30.0, 1)], 0);
        app.update();
        {
            let world = app.world();
            let sprite = world.get::<Sprite>(entity).unwrap();
            let transform = world.get::<Transform>(entity).unwrap();
            assert_eq!(sprite.custom_size, Some(Vec2::splat(10.0)));
            // Базовый кадр при нуле часов: центр куска 10×10 с углом (100, 200)
            assert_eq!(transform.translation, Vec3::new(105.0, -205.0, 0.0));
            assert_eq!(world.get::<Visibility>(entity), Some(&Visibility::Visible));
        }
        // Два тика — третий кадр последовательности из трёх по тику
        app.world_mut().resource_mut::<Clock>().ticks = 2;
        app.update();
        {
            let world = app.world();
            let sprite = world.get::<Sprite>(entity).unwrap();
            let transform = world.get::<Transform>(entity).unwrap();
            assert_eq!(sprite.custom_size, Some(Vec2::splat(30.0)));
            assert_eq!(transform.translation.x, 115.0);
            assert_eq!(transform.translation.y, -215.0);
        }
        // Тот же тик второй раз ничего не меняет: система не трогает спрайт,
        // пока кадр не сменился
        app.update();
        assert_eq!(
            app.world().get::<Sprite>(entity).unwrap().custom_size,
            Some(Vec2::splat(30.0))
        );
    }

    #[test]
    fn hidden_frame_hides_the_sprite_and_comes_back() {
        let mut frames = vec![frame(10.0, 1), frame(10.0, 1)];
        frames[1].image = None;
        let (mut app, entity) = scene(frames, 0);
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(entity),
            Some(&Visibility::Visible),
            "первый кадр показан"
        );
        app.world_mut().resource_mut::<Clock>().ticks = 1;
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(entity),
            Some(&Visibility::Hidden),
            "кадра без картинки не видно"
        );
        // Цикл вернулся к первому кадру — спрайт снова виден
        app.world_mut().resource_mut::<Clock>().ticks = 2;
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(entity),
            Some(&Visibility::Visible)
        );
    }
}
