//! Las dos únicas conexiones de la app, ambas a petición de la persona:
//! buscar una versión nueva en GitHub y estimar la ubicación por la IP.

use std::time::Duration;

pub const REPO_URL: &str = "https://github.com/DSGM-GMSD/CareOurEyes";
pub const ISSUES_URL: &str = "https://github.com/DSGM-GMSD/CareOurEyes/issues";
const LATEST_API: &str = "https://api.github.com/repos/DSGM-GMSD/CareOurEyes/releases/latest";

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("CareOurEyes/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// "v1.2.3", "1.2.3-beta" → (1, 2, 3).
pub fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let core = v.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0)))
}

pub enum Update {
    UpToDate,
    New { version: String, url: String },
}

/// Consulta la última versión publicada.
pub fn check_latest(current: &str) -> Result<Update, String> {
    let resp = agent().get(LATEST_API).set("Accept", "application/vnd.github+json").call();
    let resp = match resp {
        Ok(r) => r,
        Err(ureq::Error::Status(code, _)) => return Err(crate::i18n::tr_args("github_respondio", &[&code.to_string()])),
        Err(e) => return Err(e.kind().to_string()),
    };
    let text = resp.into_string().map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let tag = json["tag_name"].as_str().unwrap_or_default().to_string();
    let url = json["html_url"].as_str().unwrap_or(REPO_URL).to_string();
    match (parse_version(&tag), parse_version(current)) {
        (Some(latest), Some(mine)) if latest > mine => Ok(Update::New { version: tag.trim_start_matches('v').to_string(), url }),
        (Some(_), Some(_)) => Ok(Update::UpToDate),
        _ => Err(format!("tag «{tag}»")),
    }
}

/// Ubicación aproximada según la IP (ipwho.is, HTTPS). Solo al pulsar «Detectar».
pub fn locate() -> Option<(f64, f64)> {
    let text = agent()
        .get("https://ipwho.is/?fields=success,latitude,longitude")
        .call()
        .ok()?
        .into_string()
        .ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    if json["success"].as_bool() != Some(true) {
        return None;
    }
    Some((json["latitude"].as_f64()?, json["longitude"].as_f64()?))
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("2.0.0-alpha.1"), Some((2, 0, 0)));
        assert_eq!(parse_version("1.4"), Some((1, 4, 0)));
        assert!(parse_version("v2.0.0") > parse_version("1.9.9"));
        assert_eq!(parse_version("x"), None);
    }
}
