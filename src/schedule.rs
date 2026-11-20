//! Horario día/noche: horas fijas o amanecer/atardecer calculados localmente.
//!
//! El cálculo solar usa la «ecuación del amanecer» estándar (ver
//! https://en.wikipedia.org/wiki/Sunrise_equation), sin conexión a internet.

use crate::config::ScheduleConfig;
use chrono::{Datelike, Local, Offset, TimeZone, Timelike};

const DEG: f64 = std::f64::consts::PI / 180.0;

/// Amanecer y atardecer locales (minutos desde medianoche) para la fecha de hoy.
/// `None` en días polares o con coordenadas sin definir.
pub fn sun_times_today(lat: f64, lon: f64) -> Option<(i32, i32)> {
    if lat == 0.0 && lon == 0.0 {
        return None;
    }
    let now = Local::now();
    let noon = Local
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 12, 0, 0)
        .single()?;
    let jd_noon = noon.timestamp() as f64 / 86400.0 + 2440587.5;
    let (rise, set) = sunrise_sunset_jd(jd_noon, lat, lon)?;
    let offset_min = now.offset().fix().local_minus_utc() as f64 / 60.0;
    let to_local = |jd: f64| -> i32 {
        let unix = (jd - 2440587.5) * 86400.0;
        let mins = (unix / 60.0 + offset_min).rem_euclid(1440.0);
        mins.round() as i32 % 1440
    };
    Some((to_local(rise), to_local(set)))
}

/// Días julianos del amanecer y el atardecer alrededor de `jd`.
fn sunrise_sunset_jd(jd: f64, lat: f64, lon: f64) -> Option<(f64, f64)> {
    let n = (jd - 2451545.0 + 0.0008).round();
    let j_star = n - lon / 360.0;
    let m = (357.5291 + 0.98560028 * j_star).rem_euclid(360.0);
    let c = 1.9148 * (m * DEG).sin() + 0.02 * (2.0 * m * DEG).sin() + 0.0003 * (3.0 * m * DEG).sin();
    let lambda = (m + c + 180.0 + 102.9372).rem_euclid(360.0);
    let transit = 2451545.0 + j_star + 0.0053 * (m * DEG).sin() - 0.0069 * (2.0 * lambda * DEG).sin();
    let sin_decl = (lambda * DEG).sin() * (23.4397 * DEG).sin();
    let cos_decl = (1.0 - sin_decl * sin_decl).sqrt();
    let cos_w = ((-0.833 * DEG).sin() - (lat * DEG).sin() * sin_decl) / ((lat * DEG).cos() * cos_decl);
    if !(-1.0..=1.0).contains(&cos_w) {
        return None;
    }
    let w = cos_w.acos() / DEG;
    Some((transit - w / 360.0, transit + w / 360.0))
}

/// Horas de inicio del día y de la noche que tocan hoy según el horario.
pub fn day_night_starts(s: &ScheduleConfig) -> (i32, i32) {
    if s.mode == "sun" {
        if let Some((rise, set)) = sun_times_today(s.latitude, s.longitude) {
            return (rise, set);
        }
    }
    (s.day_start.rem_euclid(1440), s.night_start.rem_euclid(1440))
}

pub fn now_minutes() -> i32 {
    let n = Local::now();
    (n.hour() * 60 + n.minute()) as i32
}

/// Cuánto «de noche» es ahora: 0 = día, 1 = noche, valores intermedios durante la
/// transición. La transición empieza en la hora indicada y dura `transition_minutes`.
pub fn night_factor(s: &ScheduleConfig, minute: i32) -> f64 {
    if s.mode == "off" {
        return 0.0;
    }
    let (day, night) = day_night_starts(s);
    let fade = if s.smooth_transition { s.transition_minutes.clamp(0, 300) } else { 0 };
    // Minutos transcurridos desde cada cambio (siempre 0..1439).
    let since_night = (minute - night).rem_euclid(1440);
    let since_day = (minute - day).rem_euclid(1440);
    let is_night = since_night < since_day;
    let progress = |since: i32| if fade == 0 { 1.0 } else { (since as f64 / fade as f64).min(1.0) };
    if is_night {
        progress(since_night)
    } else {
        1.0 - progress(since_day)
    }
}

pub fn format_minutes(m: i32) -> String {
    let m = m.rem_euclid(1440);
    format!("{:02}:{:02}", m / 60, m % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual(day: i32, night: i32, fade: i32) -> ScheduleConfig {
        ScheduleConfig {
            mode: "manual".into(),
            day_start: day,
            night_start: night,
            smooth_transition: fade > 0,
            transition_minutes: fade,
            ..Default::default()
        }
    }

    #[test]
    fn fixed_hours() {
        let s = manual(7 * 60, 19 * 60, 0);
        assert_eq!(night_factor(&s, 12 * 60), 0.0);
        assert_eq!(night_factor(&s, 23 * 60), 1.0);
        assert_eq!(night_factor(&s, 3 * 60), 1.0);
    }

    #[test]
    fn fades_in_and_out() {
        let s = manual(7 * 60, 19 * 60, 60);
        assert!((night_factor(&s, 19 * 60 + 30) - 0.5).abs() < 1e-9);
        assert!((night_factor(&s, 7 * 60 + 30) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn mexico_city_sun_times_are_plausible() {
        let jd = 2461000.0; // fecha cualquiera
        let (r, s) = sunrise_sunset_jd(jd, 19.43, -99.13).unwrap();
        let day_hours = (s - r) * 24.0;
        assert!((10.5..13.5).contains(&day_hours), "{day_hours}");
    }
}
