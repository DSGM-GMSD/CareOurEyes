//! Temporizador de pausas: trabajo → (aviso) → pausa → trabajo…
//!
//! Es lógica pura: `tick` recibe cuánto tiempo pasó y qué está haciendo la
//! persona (si está ausente, si hay algo a pantalla completa) y devuelve los
//! eventos que la interfaz tiene que mostrar. Así se puede probar sin ventanas.

use crate::config::RestConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Work,
    Break { long: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Faltan `pre_break_seconds` para la pausa.
    Warn,
    BreakStarted { long: bool },
    BreakEnded,
    /// Recordatorio 20-20-20.
    BlinkStarted,
    BlinkEnded,
    /// La persona volvió tras una ausencia que contó como descanso.
    Rested,
}

/// Duración del recordatorio de parpadeo (la regla 20-20-20: 20 segundos).
pub const BLINK_MS: i64 = 20_000;

/// Lo que se sabe del equipo en este momento.
#[derive(Clone, Copy, Debug, Default)]
pub struct Context {
    /// Milisegundos desde la última tecla o movimiento del ratón.
    pub idle_ms: i64,
    /// La ventana activa ocupa toda su pantalla.
    pub fullscreen: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paused {
    Until(i64),
    Indefinitely,
}

#[derive(Clone, Debug)]
pub struct Breaks {
    pub phase: Phase,
    /// Tiempo que falta para terminar la fase actual.
    pub left_ms: i64,
    /// Sesiones de trabajo terminadas desde la última pausa larga.
    pub sessions: i32,
    pub delays_used: i32,
    pub paused: Option<Paused>,
    warned: bool,
    blink_left_ms: i64,
    /// Tiempo que queda mostrando el recordatorio de parpadeo (0 = no se muestra).
    pub blink_showing_ms: i64,
    /// La ausencia actual ya alcanza para contar como descanso.
    rested: bool,
    /// La pausa espera a que termine una pantalla completa.
    pub held: bool,
}

fn work_ms(c: &RestConfig) -> i64 {
    c.work_minutes.clamp(1, 240) as i64 * 60_000
}

fn long_ms(c: &RestConfig) -> i64 {
    c.long_break_minutes.clamp(1, 120) as i64 * 60_000
}

fn blink_every_ms(c: &RestConfig) -> i64 {
    c.blink_interval_minutes.clamp(1, 120) as i64 * 60_000
}

impl Breaks {
    pub fn new(c: &RestConfig) -> Self {
        Self {
            phase: Phase::Work,
            left_ms: work_ms(c),
            sessions: 0,
            delays_used: 0,
            paused: None,
            warned: false,
            blink_left_ms: blink_every_ms(c),
            blink_showing_ms: 0,
            rested: false,
            held: false,
        }
    }

    /// Empieza a contar el trabajo desde cero.
    pub fn restart_work(&mut self, c: &RestConfig) {
        self.phase = Phase::Work;
        self.left_ms = work_ms(c);
        self.warned = false;
        self.blink_left_ms = blink_every_ms(c);
        self.blink_showing_ms = 0;
        self.delays_used = 0;
        self.rested = false;
        self.held = false;
    }

    /// Vuelve a contar el intervalo del recordatorio de parpadeo.
    pub fn reset_blink(&mut self, c: &RestConfig) {
        self.blink_left_ms = blink_every_ms(c);
        self.blink_showing_ms = 0;
    }

    /// ¿La próxima pausa será larga?
    pub fn next_is_long(&self, c: &RestConfig) -> bool {
        c.pomodoro && self.sessions + 1 >= c.long_break_every.clamp(2, 12)
    }

    pub fn can_delay(&self, c: &RestConfig) -> bool {
        c.allow_delay && c.break_style != "strict" && self.delays_used < c.delay_times_max.clamp(1, 10)
    }

    fn start_break(&mut self, c: &RestConfig) -> Event {
        let long = self.next_is_long(c);
        self.phase = Phase::Break { long };
        self.left_ms = if long { long_ms(c) } else { c.short_break_ms() };
        self.blink_showing_ms = 0;
        self.held = false;
        Event::BreakStarted { long }
    }

    fn finish_break(&mut self, c: &RestConfig) -> Event {
        if let Phase::Break { long } = self.phase {
            if long {
                self.sessions = 0;
            } else {
                self.sessions += 1;
            }
        }
        self.restart_work(c);
        Event::BreakEnded
    }

    /// «Descansar ahora».
    pub fn take_break_now(&mut self, c: &RestConfig) -> Event {
        self.paused = None;
        self.start_break(c)
    }

    /// Posponer la pausa (desde el aviso o durante una pausa que se puede saltar).
    pub fn postpone(&mut self, c: &RestConfig) -> bool {
        if !self.can_delay(c) {
            return false;
        }
        self.delays_used += 1;
        self.phase = Phase::Work;
        self.left_ms = c.delay_minutes.clamp(1, 60) as i64 * 60_000;
        self.warned = false;
        self.held = false;
        true
    }

    /// Saltar: durante una pausa la termina; en el trabajo salta la siguiente.
    /// Cuenta como una sesión hecha, para que el ciclo Pomodoro siga su curso.
    pub fn skip(&mut self, c: &RestConfig) -> Event {
        if matches!(self.phase, Phase::Work) {
            self.phase = Phase::Break { long: self.next_is_long(c) };
        }
        self.finish_break(c)
    }

    pub fn pause(&mut self, minutes: Option<i64>) {
        self.paused = Some(match minutes {
            Some(m) => Paused::Until(m * 60_000),
            None => Paused::Indefinitely,
        });
        self.blink_showing_ms = 0;
    }

    pub fn resume(&mut self) {
        self.paused = None;
    }

    pub fn tick(&mut self, dt_ms: i64, c: &RestConfig, ctx: Context) -> Vec<Event> {
        let mut ev = Vec::new();
        let dt = dt_ms.max(0);

        if let Some(p) = self.paused {
            if let Paused::Until(left) = p {
                let left = left - dt;
                self.paused = if left <= 0 { None } else { Some(Paused::Until(left)) };
            }
            return ev;
        }

        // El recordatorio de parpadeo se va solo a los 20 s.
        if self.blink_showing_ms > 0 {
            self.blink_showing_ms -= dt;
            if self.blink_showing_ms <= 0 {
                self.blink_showing_ms = 0;
                ev.push(Event::BlinkEnded);
            }
        }

        match self.phase {
            Phase::Work => {
                // Ausencia: el reloj se detiene; si dura lo que una pausa corta,
                // al volver se empieza de cero (ya descansó).
                if c.pause_when_idle && ctx.idle_ms >= c.idle_ms() {
                    if ctx.idle_ms >= c.short_break_ms() {
                        self.rested = true;
                    }
                    return ev;
                }
                if self.rested {
                    self.restart_work(c);
                    ev.push(Event::Rested);
                    return ev;
                }

                self.left_ms -= dt;

                if c.blink_enabled && self.blink_showing_ms == 0 {
                    self.blink_left_ms -= dt;
                    if self.blink_left_ms <= 0 {
                        self.blink_left_ms = blink_every_ms(c);
                        // No hace falta si la pausa está a punto de llegar.
                        if self.left_ms > 60_000 && !(c.do_not_disturb_fullscreen && ctx.fullscreen) {
                            self.blink_showing_ms = BLINK_MS;
                            ev.push(Event::BlinkStarted);
                        }
                    }
                }

                let warn = c.pre_break_seconds.clamp(0, 300) as i64 * 1000;
                if !self.warned && warn > 0 && self.left_ms <= warn && self.left_ms > 0 {
                    self.warned = true;
                    if !(c.do_not_disturb_fullscreen && ctx.fullscreen) {
                        ev.push(Event::Warn);
                    }
                }

                if self.left_ms <= 0 {
                    if c.do_not_disturb_fullscreen && ctx.fullscreen {
                        // Espera a que termine la pantalla completa.
                        self.left_ms = 0;
                        self.held = true;
                    } else {
                        if self.blink_showing_ms > 0 {
                            self.blink_showing_ms = 0;
                            ev.push(Event::BlinkEnded);
                        }
                        ev.push(self.start_break(c));
                    }
                }
            }
            Phase::Break { .. } => {
                self.left_ms -= dt;
                if self.left_ms <= 0 {
                    ev.push(self.finish_break(c));
                }
            }
        }
        ev
    }
}

/// "mm:ss", o "h:mm:ss" si pasa de una hora.
pub fn clock(ms: i64) -> String {
    let s = (ms.max(0) + 999) / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> RestConfig {
        RestConfig {
            enabled: true,
            work_minutes: 1,
            short_break_minutes: 1,
            long_break_minutes: 2,
            long_break_every: 2,
            pomodoro: true,
            pre_break_seconds: 10,
            pause_when_idle: true,
            pause_idle_amount: 30,
            pause_idle_unit: "seconds".into(),
            ..Default::default()
        }
    }

    fn run(b: &mut Breaks, c: &RestConfig, secs: i64, ctx: Context) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..secs {
            all.extend(b.tick(1000, c, ctx));
        }
        all
    }

    #[test]
    fn work_warn_break_and_long_break() {
        let c = cfg();
        let mut b = Breaks::new(&c);
        let ev = run(&mut b, &c, 60, Context::default());
        assert_eq!(ev, vec![Event::Warn, Event::BreakStarted { long: false }]);
        let ev = run(&mut b, &c, 60, Context::default());
        assert_eq!(ev, vec![Event::BreakEnded]);
        assert_eq!(b.sessions, 1);
        // La segunda es larga (cada 2 sesiones) y luego el contador vuelve a 0.
        let ev = run(&mut b, &c, 60, Context::default());
        assert!(ev.contains(&Event::BreakStarted { long: true }));
        run(&mut b, &c, 120, Context::default());
        assert_eq!(b.sessions, 0);
        assert_eq!(b.phase, Phase::Work);
    }

    #[test]
    fn idle_stops_the_clock_and_long_absence_counts_as_rest() {
        let c = cfg();
        let mut b = Breaks::new(&c);
        run(&mut b, &c, 20, Context::default());
        let left = b.left_ms;
        // 40 s sin tocar nada: el reloj se detiene (no llega a una pausa corta).
        b.tick(1000, &c, Context { idle_ms: 40_000, ..Default::default() });
        assert_eq!(b.left_ms, left);
        // 2 min ausente: al volver empieza de cero.
        b.tick(1000, &c, Context { idle_ms: 120_000, ..Default::default() });
        let ev = b.tick(1000, &c, Context::default());
        assert_eq!(ev, vec![Event::Rested]);
        assert_eq!(b.left_ms, 60_000);
    }

    #[test]
    fn fullscreen_holds_the_break() {
        let c = RestConfig { do_not_disturb_fullscreen: true, ..cfg() };
        let mut b = Breaks::new(&c);
        let full = Context { fullscreen: true, ..Default::default() };
        let ev = run(&mut b, &c, 70, full);
        assert!(ev.is_empty());
        assert!(b.held);
        let ev = b.tick(1000, &c, Context::default());
        assert_eq!(ev, vec![Event::BreakStarted { long: false }]);
    }

    #[test]
    fn postpone_is_limited() {
        let c = RestConfig { delay_times_max: 2, ..cfg() };
        let mut b = Breaks::new(&c);
        assert!(b.postpone(&c));
        assert!(b.postpone(&c));
        assert!(!b.postpone(&c));
        let strict = RestConfig { break_style: "strict".into(), ..cfg() };
        let mut b = Breaks::new(&strict);
        assert!(!b.postpone(&strict));
    }

    #[test]
    fn pause_for_a_while() {
        let c = cfg();
        let mut b = Breaks::new(&c);
        b.pause(Some(1));
        run(&mut b, &c, 59, Context::default());
        assert!(b.paused.is_some());
        assert_eq!(b.left_ms, 60_000);
        run(&mut b, &c, 1, Context::default());
        assert!(b.paused.is_none());
    }

    #[test]
    fn clock_format() {
        assert_eq!(clock(65_000), "01:05");
        assert_eq!(clock(3_661_000), "1:01:01");
        assert_eq!(clock(400), "00:01");
    }
}
