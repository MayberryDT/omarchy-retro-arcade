use crate::{
    ai::{Difficulty, Search},
    rules::{Phase, Point, Weapon, DT},
    storage::{self, Mode, Save},
};
use eframe::egui::{self, Align2, FontId, Key, Pos2, Rect, Sense, Stroke, Vec2};
use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub struct App {
    save: Save,
    path: PathBuf,
    blocked: bool,
    notice: Option<String>,
    paused: bool,
    settings: bool,
    restart: bool,
    enabled: bool,
    armed: bool,
    leave: bool,
    search: Option<Search>,
    last: Instant,
    accumulator: f64,
    movement_clock: f64,
    theme: omarchy_chess::theme::Theme,
    themed: Instant,
}
impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
impl App {
    pub fn new() -> Self {
        Self::from_path(storage::path())
    }
    fn from_path(path: PathBuf) -> Self {
        let (save, notice) = match storage::load(&path) {
            Ok(s) => (s, None),
            Err(e) => (Save::default(), Some(e)),
        };
        Self {
            save,
            path,
            blocked: notice.is_some(),
            notice,
            paused: true,
            settings: false,
            restart: false,
            enabled: true,
            armed: false,
            leave: false,
            search: None,
            last: Instant::now(),
            accumulator: 0.0,
            movement_clock: 0.0,
            theme: omarchy_chess::theme::Theme::load(),
            themed: Instant::now(),
        }
    }
    fn persist(&mut self) {
        if self.blocked {
            return;
        }
        self.save.observe();
        if let Err(e) = storage::write(&self.path, &self.save) {
            self.notice = Some(format!("Could not save: {e}"));
        }
    }
    pub fn suspend(&mut self) {
        self.paused = true;
        self.armed = false;
        self.accumulator = 0.0;
        self.movement_clock = 0.0;
        self.search = None;
        self.persist();
    }
    pub fn set_input_enabled(&mut self, enabled: bool) {
        if self.enabled && !enabled {
            self.suspend();
        }
        self.enabled = enabled;
    }
    pub fn finished(&mut self) -> bool {
        self.leave
    }
    fn ai_turn(&self) -> bool {
        self.save.mode == Mode::Solo && self.save.game.active() == 1
    }
    fn begin_match(&mut self) {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        self.save.new_match(seed);
        self.search = None;
        self.restart = false;
        self.paused = false;
        self.armed = false;
        self.accumulator = 0.0;
        self.persist();
    }
    fn field(&self, ui: &mut egui::Ui) -> (Rect, f32, egui::Response) {
        let available = ui.available_size();
        let scale = (available.x / 1000.0).min(available.y / 600.0).max(0.01);
        let (outer, _) = ui.allocate_exact_size(available, Sense::hover());
        let rect = Rect::from_center_size(outer.center(), Vec2::new(1000.0, 600.0) * scale);
        let response = ui.interact(rect, ui.id().with("battlefield"), Sense::drag());
        let p = ui.painter_at(rect);
        let pt = |x: f64, y: f64| {
            Pos2::new(
                rect.left() + x as f32 * scale,
                rect.top() + y as f32 * scale,
            )
        };
        let fg = self.theme.foreground;
        let accent = self.theme.accent;
        let bg = self.theme.background;
        p.rect_filled(rect, 0.0, bg);
        // Survey lines give height and distance references without predicting a shot.
        for x in (0..=1000).step_by(100) {
            p.line_segment(
                [pt(x as f64, 0.0), pt(x as f64, 600.0)],
                Stroke::new(0.5_f32, fg.gamma_multiply(0.09)),
            );
        }
        for y in (0..=600).step_by(100) {
            p.line_segment(
                [pt(0.0, y as f64), pt(1000.0, y as f64)],
                Stroke::new(0.5_f32, fg.gamma_multiply(0.09)),
            );
        }
        let heights = self.save.game.terrain().heights();
        for x in 0..1000 {
            p.add(egui::Shape::convex_polygon(
                vec![
                    pt(x as f64, heights[x]),
                    pt(x as f64 + 1.0, heights[x + 1]),
                    pt(x as f64 + 1.0, 600.0),
                    pt(x as f64, 600.0),
                ],
                accent.gamma_multiply(0.24),
                Stroke::NONE,
            ));
            p.line_segment(
                [pt(x as f64, heights[x]), pt(x as f64 + 1.0, heights[x + 1])],
                Stroke::new(1.4_f32, accent),
            );
        }
        for trace in self.save.game.traces() {
            for pair in trace.windows(2).step_by(3) {
                p.line_segment(
                    [pt(pair[0].x, pair[0].y), pt(pair[1].x, pair[1].y)],
                    Stroke::new(1.0_f32, fg.gamma_multiply(0.25)),
                );
            }
        }
        for (i, tank) in self.save.game.tanks().iter().enumerate() {
            let x = tank.position.x;
            let y = tank.position.y;
            let ink = if i == 0 { accent } else { fg };
            p.rect_filled(
                Rect::from_min_max(pt(x - 12.0, y - 7.0), pt(x + 12.0, y)),
                0.0,
                ink,
            );
            p.rect_filled(
                Rect::from_min_max(pt(x - 9.0, y - 14.0), pt(x + 9.0, y - 7.0)),
                0.0,
                ink,
            );
            // Track cuts and a second turret stripe distinguish the silhouettes.
            for dx in [-8.0, 0.0, 8.0] {
                p.line_segment(
                    [pt(x + dx, y - 5.0), pt(x + dx, y - 1.0)],
                    Stroke::new(scale, bg),
                );
            }
            if i == 1 {
                p.line_segment(
                    [pt(x - 6.0, y - 12.0), pt(x + 6.0, y - 12.0)],
                    Stroke::new(scale, bg),
                );
            }
            let a = f64::from(tank.angle).to_radians();
            p.line_segment(
                [
                    pt(x, y - 7.0),
                    pt(x + 20.0 * a.cos(), y - 7.0 - 20.0 * a.sin()),
                ],
                Stroke::new(3.0 * scale, ink),
            );
            p.text(
                pt(x, y - 32.0),
                Align2::CENTER_CENTER,
                format!("P{} · {}", i + 1, tank.health),
                FontId::monospace((14.0 * scale).max(10.0)),
                ink,
            );
            if i == self.save.game.active() {
                p.add(egui::Shape::convex_polygon(
                    vec![
                        pt(x - 5.0, y - 56.0),
                        pt(x + 5.0, y - 56.0),
                        pt(x, y - 48.0),
                    ],
                    ink,
                    Stroke::NONE,
                ));
            }
        }
        if let Some(shot) = self.save.game.projectile() {
            let pos = shot.position;
            if pos.y < 0.0 {
                p.text(
                    pt(pos.x, 12.0),
                    Align2::CENTER_TOP,
                    format!("↑ {:.0}", -pos.y),
                    FontId::monospace(13.0),
                    fg,
                );
            } else {
                p.circle_filled(pt(pos.x, pos.y), (3.0 * scale).max(2.0), fg);
            }
        }
        (rect, scale, response)
    }
    pub fn frame(&mut self, ctx: &egui::Context) {
        let elapsed = self.last.elapsed().as_secs_f64();
        self.last = Instant::now();
        if self.themed.elapsed() > Duration::from_secs(2) {
            self.theme = omarchy_chess::theme::Theme::load();
            self.themed = Instant::now();
        }
        let mut visuals = if self.theme.light() {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        visuals.panel_fill = self.theme.background;
        visuals.window_fill = self.theme.background;
        visuals.override_text_color = Some(self.theme.foreground);
        visuals.selection.bg_fill = self.theme.accent;
        ctx.set_visuals(visuals);
        arcade_presentation::apply(ctx);
        let focused = ctx.input(|i| i.focused);
        if (!focused || elapsed > 0.25) && !self.paused {
            self.suspend();
        }
        let held = ctx.input(|i| {
            i.pointer.any_down()
                || [
                    Key::Space,
                    Key::A,
                    Key::D,
                    Key::ArrowUp,
                    Key::ArrowDown,
                    Key::ArrowLeft,
                    Key::ArrowRight,
                    Key::Num1,
                    Key::Num2,
                    Key::Num3,
                ]
                .iter()
                .any(|k| i.key_down(*k))
        });
        if !self.armed && !held && focused && self.enabled {
            self.armed = true;
        }
        if self.enabled && focused && ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.paused {
                self.settings = false;
                self.restart = false;
                self.paused = false;
                self.armed = false;
            } else {
                self.suspend();
            }
        }
        if self.enabled && focused && ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::Comma)) {
            self.suspend();
            self.settings = true;
        }
        let interactive = self.enabled && focused && !self.paused && self.armed;
        let human = interactive && !self.ai_turn() && self.save.game.phase() == Phase::Aiming;
        let mut fire = false;
        let mut movement = 0;
        egui::TopBottomPanel::top("tanks-header").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("TANKS");
                ui.label(format!("ROUND {} · FIRST TO TWO", self.save.game.round()));
                let wins = self.save.game.wins();
                ui.label(format!("P1 {} : {} P2", wins[0], wins[1]));
                ui.label(format!("WIND {:+.0}", self.save.game.wind()));
                if ui
                    .add_enabled(
                        self.enabled,
                        egui::Button::new(if self.paused { "Resume" } else { "Pause" }),
                    )
                    .clicked()
                {
                    if self.paused {
                        self.paused = false;
                        self.settings = false;
                        self.restart = false;
                        self.armed = false;
                    } else {
                        self.suspend();
                    }
                }
                if ui.button("Settings").clicked() {
                    self.suspend();
                    self.settings = true;
                }
                if ui.button("Back to Arcade").clicked() {
                    self.suspend();
                    self.leave = true;
                }
            });
            if let Some(notice) = self.notice.clone() {
                ui.label(notice);
            }
        });
        egui::TopBottomPanel::bottom("tanks-controls").show(ctx, |ui| {
            let active = self.save.game.active();
            let tank = self.save.game.tanks()[active].clone();
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "P{} · HEALTH {} · FUEL {:.0}",
                    active + 1,
                    tank.health,
                    tank.fuel
                ));
                ui.add_enabled_ui(human, |ui| {
                    let mut angle = tank.angle;
                    let mut power = tank.power;
                    ui.label("Angle");
                    ui.add(egui::DragValue::new(&mut angle).range(5..=175).suffix("°"));
                    ui.label("Power");
                    let slider = ui.add(egui::Slider::new(&mut power, 1..=100));
                    if slider.hovered() {
                        let scroll = ctx.input_mut(|i| {
                            let d = i.smooth_scroll_delta.y;
                            i.smooth_scroll_delta = Vec2::ZERO;
                            d
                        });
                        if scroll != 0.0 {
                            power = (i32::from(power) + if scroll > 0.0 { 1 } else { -1 })
                                .clamp(1, 100) as u16;
                        }
                    }
                    if angle != tank.angle || power != tank.power {
                        let _ = self.save.game.aim(angle, power);
                    }
                    let left = ui.button("← Move").on_hover_text("A · hold to move");
                    let right = ui.button("Move →").on_hover_text("D · hold to move");
                    movement = i8::from(right.is_pointer_button_down_on())
                        - i8::from(left.is_pointer_button_down_on());
                    fire = ui.button("FIRE · Space").clicked();
                });
            });
            ui.horizontal_wrapped(|ui| {
                for (weapon, label) in [
                    (Weapon::Shell, "1 Shell ∞".to_string()),
                    (Weapon::Heavy, format!("2 Heavy · {}", tank.heavy)),
                    (Weapon::Digger, format!("3 Digger · {}", tank.diggers)),
                ] {
                    if ui
                        .add_enabled(
                            human && tank.available(weapon),
                            egui::Button::new(label).selected(tank.weapon == weapon),
                        )
                        .clicked()
                    {
                        let _ = self.save.game.select(weapon);
                    }
                }
                ui.label("↑↓ angle · ←→ power · A/D move");
            });
            match self.save.game.phase() {
                Phase::Ready => {
                    ui.horizontal(|ui| {
                        ui.label(format!("Player {} — ready?", active + 1));
                        if ui
                            .add_enabled(interactive, egui::Button::new("Ready · Enter"))
                            .clicked()
                            || (interactive && ctx.input(|i| i.key_pressed(Key::Enter)))
                        {
                            let _ = self.save.game.ready();
                            self.armed = false;
                        }
                    });
                }
                Phase::Flying => {
                    ui.label("SHOT IN FLIGHT");
                }
                Phase::Aiming if self.ai_turn() => {
                    ui.label("Computer is choosing a shot…");
                }
                Phase::RoundOver { winner } => {
                    ui.horizontal(|ui| {
                        ui.label(
                            winner.map_or("Both tanks eliminated — drawn round".into(), |i| {
                                format!("Player {} wins the round", i + 1)
                            }),
                        );
                        if ui
                            .add_enabled(interactive, egui::Button::new("Next round · Enter"))
                            .clicked()
                            || (interactive && ctx.input(|i| i.key_pressed(Key::Enter)))
                        {
                            let _ = self.save.game.next_round();
                            self.armed = false;
                            self.persist();
                        }
                    });
                }
                Phase::MatchOver { winner } => {
                    ui.horizontal(|ui| {
                        ui.label(format!("PLAYER {} WINS THE MATCH", winner + 1));
                        if ui
                            .add_enabled(interactive, egui::Button::new("Play again"))
                            .clicked()
                        {
                            self.begin_match();
                        }
                    });
                }
                _ => {}
            }
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            let (rect, scale, response) = self.field(ui);
            if human && response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let tank = &self.save.game.tanks()[self.save.game.active()];
                    let point = Point {
                        x: f64::from((pos.x - rect.left()) / scale),
                        y: f64::from((pos.y - rect.top()) / scale),
                    };
                    let centre = tank.centre();
                    let angle = (centre.y - point.y)
                        .atan2(point.x - centre.x)
                        .to_degrees()
                        .clamp(5.0, 175.0) as u16;
                    let _ = self.save.game.aim(angle, tank.power);
                }
            }
        });
        if self.paused {
            egui::Window::new(if self.settings{"Tanks settings"}else{"Paused"}).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER,Vec2::ZERO).show(ctx,|ui| {
                ui.label("Match preserved. Resume when ready.");
                ui.label(format!("Solo: {} wins / {} losses · Local: {} matches",self.save.solo_wins,self.save.solo_losses,self.save.local_matches));
                if self.settings {
                    ui.checkbox(&mut self.save.reduced_effects,"Reduced effects");
                    ui.label("This preview is silent. Sound and impact animation are still in development.");
                    ui.label("Choose mode and difficulty for a new match:");
                    // Mode remains tied to this match until a confirmed replacement.
                }
                if self.restart {
                    ui.label("Discard this match and start again?");
                    for (mode,difficulty,label) in [(Mode::Solo,Difficulty::Easy,"Solo — Easy"),(Mode::Solo,Difficulty::Normal,"Solo — Normal"),(Mode::Local,Difficulty::Normal,"Two local players")] {
                        if ui.button(label).clicked() {self.save.mode=mode;self.save.difficulty=difficulty;self.begin_match();}
                    }
                    if ui.button("Cancel").clicked(){self.restart=false;}
                } else if ui.button("New match…").clicked(){self.restart=true;}
                if self.blocked {
                    ui.label("Saving is disabled to protect the original file.");
                    if ui.button("Archive original and reset").clicked() {match storage::archive(&self.path) {Ok(path)=>{self.blocked=false;self.save=Save::default();self.notice=Some(format!("Original archived to {}",path.display()));self.persist();},Err(e)=>self.notice=Some(format!("Archive failed; original retained: {e}"))}}
                }
                if ui.button("Resume · Escape").clicked(){self.paused=false;self.settings=false;self.restart=false;self.armed=false;self.persist();}
                if ui.button("Back to Arcade").clicked(){self.suspend();self.leave=true;}
            });
        }
        if human && self.armed && !self.paused && !ctx.wants_keyboard_input() {
            let t = self.save.game.tanks()[self.save.game.active()].clone();
            let (da, dp, m, f, weapon) = ctx.input(|i| {
                (
                    i32::from(i.key_pressed(Key::ArrowUp))
                        - i32::from(i.key_pressed(Key::ArrowDown)),
                    i32::from(i.key_pressed(Key::ArrowRight))
                        - i32::from(i.key_pressed(Key::ArrowLeft)),
                    i8::from(i.key_down(Key::D)) - i8::from(i.key_down(Key::A)),
                    i.key_pressed(Key::Space),
                    if i.key_pressed(Key::Num1) {
                        Some(Weapon::Shell)
                    } else if i.key_pressed(Key::Num2) {
                        Some(Weapon::Heavy)
                    } else if i.key_pressed(Key::Num3) {
                        Some(Weapon::Digger)
                    } else {
                        None
                    },
                )
            });
            if da != 0 || dp != 0 {
                let _ = self.save.game.aim(
                    (i32::from(t.angle) + da).clamp(5, 175) as u16,
                    (i32::from(t.power) + dp).clamp(1, 100) as u16,
                );
            }
            if m != 0 {
                movement = m;
            }
            fire |= f;
            if let Some(w) = weapon {
                let _ = self.save.game.select(w);
            }
        }
        if interactive && !self.paused && self.armed {
            if human {
                if movement != 0 {
                    self.movement_clock += elapsed.min(0.25) * 30.0;
                    while self.movement_clock >= 1.0 {
                        self.movement_clock -= 1.0;
                        if let Err(e) = self.save.game.move_one(movement > 0) {
                            self.notice = Some(format!("Cannot move: {e:?}"));
                            self.movement_clock = 0.0;
                            break;
                        }
                    }
                } else {
                    self.movement_clock = 0.0;
                }
                if fire {
                    let _ = self.save.game.fire();
                    self.armed = false;
                }
            }
            if self.ai_turn() {
                if self.save.game.phase() == Phase::Ready {
                    let _ = self.save.game.ready();
                }
                if self.save.game.phase() == Phase::Aiming {
                    if self.search.is_none() {
                        self.search =
                            Search::new(&self.save.game, self.save.difficulty, self.save.ai_seed);
                    }
                    if let Some(search) = &mut self.search {
                        if search.advance(1024) {
                            if search.apply(&mut self.save.game).is_ok() {
                                self.save.ai_seed = search.next_seed();
                            }
                            self.search = None;
                        }
                    }
                }
            }
        }
        if !self.paused && self.enabled && focused {
            self.accumulator += elapsed.min(0.25);
            while self.accumulator >= DT {
                self.accumulator -= DT;
                let phase = self.save.game.phase();
                self.save.game.tick();
                if phase == Phase::Flying && self.save.game.phase() != Phase::Flying {
                    self.armed = false;
                    self.movement_clock = 0.0;
                    self.persist();
                }
            }
        } else {
            self.accumulator = 0.0;
        }
        ctx.request_repaint_after(Duration::from_millis(8));
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.frame(ctx);
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.suspend();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 800.0))),
            focused: true,
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.frame(ctx));
    }
    fn key(app: &mut App, ctx: &egui::Context, key: Key) {
        frame(
            app,
            ctx,
            vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        frame(
            app,
            ctx,
            vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    #[test]
    fn keyboard_handover_pause_host_blocking_and_exact_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tanks.json");
        let mut app = App::from_path(path.clone());
        app.save.mode = Mode::Local;
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        key(&mut app, &ctx, Key::Space);
        assert_eq!(app.save.game.phase(), Phase::Ready);
        key(&mut app, &ctx, Key::Escape);
        key(&mut app, &ctx, Key::Enter);
        assert_eq!(app.save.game.phase(), Phase::Aiming);
        key(&mut app, &ctx, Key::Space);
        assert_eq!(app.save.game.phase(), Phase::Flying);
        key(&mut app, &ctx, Key::Escape);
        let snapshot = app.save.clone();
        key(&mut app, &ctx, Key::Space);
        assert_eq!(app.save, snapshot);
        app.set_input_enabled(false);
        key(&mut app, &ctx, Key::Escape);
        assert!(app.paused);
        assert_eq!(app.save, snapshot);
        app.suspend();
        let reopened = App::from_path(path);
        assert!(reopened.paused);
        assert_eq!(reopened.save, snapshot);
    }
    #[test]
    fn mouse_fire_commits_once_and_paused_clicks_cannot_fire() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::from_path(dir.path().join("tanks.json"));
        app.save.mode = Mode::Local;
        app.save.game.ready().unwrap();
        app.paused = false;
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 800.0))),
            focused: true,
            ..Default::default()
        };
        let output = ctx.run(input, |ctx| app.frame(ctx));
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().contains("FIRE · Space") => {
                    Some(text.pos + Vec2::splat(4.0))
                }
                _ => None,
            })
            .expect("visible fire control");
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(app.save.game.phase(), Phase::Flying);
        app.suspend();
        let before = app.save.clone();
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert_eq!(app.save, before);
    }
    #[test]
    fn rejected_save_stays_untouched_through_play_and_exit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tanks.json");
        std::fs::write(&path, b"future-save").unwrap();
        let mut app = App::from_path(path.clone());
        assert!(app.blocked);
        app.begin_match();
        app.suspend();
        assert_eq!(std::fs::read(path).unwrap(), b"future-save");
    }
}
