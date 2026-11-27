//! Las dos únicas conexiones de la app, ambas a petición de la persona:
//! buscar una versión nueva en GitHub y estimar la ubicación por la IP.

use std::time::Duration;

pub const REPO_URL: &str = "https://github.com/DSGM-GMSD/CareOurEyes";
pub const ISSUES_URL: &str = "https://github.com/DSGM-GMSD/CareOurEyes/issues";
/// Lista de releases (incluye pre-releases; `/releases/latest` da 404 si solo hay de esos).
const RELEASES_API: &str = "https://api.github.com/repos/DSGM-GMSD/CareOurEyes/releases?per_page=20";

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

/// Versión comparable: número, y luego la etapa. «2.0.0» va después de
/// «2.0.0-rc.1», que va después de «2.0.0-beta.2», etc.
#[derive(Debug, PartialEq, Eq)]
pub struct Version {
    core: (u32, u32, u32),
    /// `None` = versión estable (mayor que cualquier pre-release del mismo número).
    pre: Option<(u8, u32)>,
}

impl Version {
    pub fn is_prerelease(&self) -> bool {
        self.pre.is_some()
    }
}

/// "v2.0.0-beta.1" → 2.0.0, etapa beta, número 1.
pub fn version(v: &str) -> Option<Version> {
    let core = parse_version(v)?;
    let v = v.trim().trim_start_matches(['v', 'V']);
    let pre = match v.split_once('-') {
        None => None,
        Some((_, tail)) => {
            let tail = tail.split('+').next().unwrap_or("").to_ascii_lowercase();
            let stage = if tail.starts_with("alpha") {
                0
            } else if tail.starts_with("beta") {
                1
            } else if tail.starts_with("rc") {
                2
            } else {
                0
            };
            let n = tail.rsplit(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse().ok()).unwrap_or(0);
            Some((stage, n))
        }
    };
    Some(Version { core, pre })
}

impl Version {
    /// La estable va después de todas sus pre-releases.
    fn key(&self) -> ((u32, u32, u32), u8, u32) {
        match self.pre {
            None => (self.core, u8::MAX, u32::MAX),
            Some((stage, n)) => (self.core, stage, n),
        }
    }
}

/// ¿`a` es más nueva que `b`?
pub fn newer(a: &Version, b: &Version) -> bool {
    a.key() > b.key()
}

pub enum Update {
    UpToDate,
    New { version: String, url: String },
}

/// Consulta la última versión publicada. Quien usa una versión estable solo
/// recibe avisos de estables; quien usa una alfa, beta o rc, también de esas.
pub fn check_latest(current: &str) -> Result<Update, String> {
    let resp = agent().get(RELEASES_API).set("Accept", "application/vnd.github+json").call();
    let resp = match resp {
        Ok(r) => r,
        Err(ureq::Error::Status(code, _)) => return Err(crate::i18n::tr_args("github_respondio", &[&code.to_string()])),
        Err(e) => return Err(e.kind().to_string()),
    };
    let text = resp.into_string().map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mine = version(current).ok_or_else(|| format!("versión «{current}»"))?;
    let best = json
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false))
        .filter_map(|r| {
            let tag = r["tag_name"].as_str()?.to_string();
            let v = version(&tag)?;
            let url = r["html_url"].as_str().unwrap_or(REPO_URL).to_string();
            Some((v, tag, url))
        })
        .filter(|(v, _, _)| mine.is_prerelease() || !v.is_prerelease())
        .max_by(|a, b| a.0.key().cmp(&b.0.key()));
    match best {
        Some((v, tag, url)) if newer(&v, &mine) => Ok(Update::New { version: tag.trim_start_matches('v').to_string(), url }),
        _ => Ok(Update::UpToDate),
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
    use super::{newer, parse_version, version};

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("2.0.0-alpha.1"), Some((2, 0, 0)));
        assert_eq!(parse_version("1.4"), Some((1, 4, 0)));
        assert!(parse_version("v2.0.0") > parse_version("1.9.9"));
        assert_eq!(parse_version("x"), None);
        let v = |s| version(s).unwrap();
        assert!(newer(&v("2.0.0-beta.1"), &v("2.0.0-alpha.2")));
        assert!(newer(&v("v2.0.0-beta.2"), &v("2.0.0-beta.1")));
        assert!(newer(&v("2.0.0-rc.1"), &v("2.0.0-beta.9")));
        assert!(newer(&v("2.0.0"), &v("2.0.0-rc.3")));
        assert!(newer(&v("2.0.1-alpha.1"), &v("2.0.0")));
        assert!(!newer(&v("2.0.0-beta.1"), &v("2.0.0-beta.1")));
        assert!(v("2.0.0-beta.1").is_prerelease() && !v("2.0.0").is_prerelease());
    }
}
