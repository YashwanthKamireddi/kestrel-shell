//! Region formats (dates, numbers, money, measurements, paper) as apps read them: the LC_*
//! values in /etc/locale.conf, set through systemd-localed. A region's locale is generated
//! first (locale.gen, locale-gen) and only used once it is there: a locale.conf naming a
//! locale that does not exist breaks apps (it happened here once).

/// One region: its locale name ("en_IN", "en_GB.UTF-8"), the locale.gen line that makes it,
/// and what people call it ("English (New Zealand)").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    pub locale: String,
    pub line: String,
    pub name: String,
}

const KEYS: [&str; 6] = [
    "LC_TIME",
    "LC_NUMERIC",
    "LC_MONETARY",
    "LC_MEASUREMENT",
    "LC_PAPER",
    "LC_ADDRESS",
];

/// "English (New Zealand)" from glibc's locale source; the locale name when it has none.
fn name_of(locale: &str) -> String {
    let base = locale.split(['.', '@']).next().unwrap_or(locale);
    let text =
        std::fs::read_to_string(format!("/usr/share/i18n/locales/{base}")).unwrap_or_default();
    let field = |k: &str| {
        text.lines()
            .find(|l| l.trim_start().starts_with(k))
            .and_then(|l| l.split('"').nth(1))
            .map(str::to_string)
    };
    match (field("language"), field("territory")) {
        (Some(l), Some(t)) => format!("{l} ({t})"),
        (Some(l), None) => l,
        _ => locale.to_string(),
    }
}

/// Every UTF-8 region glibc can make, by name.
pub fn supported() -> Vec<Region> {
    let text = std::fs::read_to_string("/usr/share/i18n/SUPPORTED").unwrap_or_default();
    let mut v: Vec<Region> = text
        .lines()
        .filter_map(|l| {
            let (loc, charset) = l.split_once(' ')?;
            (charset.trim() == "UTF-8" && safe(loc)).then(|| Region {
                locale: loc.to_string(),
                line: l.trim().to_string(),
                name: name_of(loc),
            })
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name).then(a.locale.cmp(&b.locale)));
    v
}

/// Only locale names go into the shell script (letters, digits, `_.@-`).
fn safe(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '@' | '-'))
}

fn locale_conf() -> Vec<(String, String)> {
    std::fs::read_to_string("/etc/locale.conf")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((k.trim().to_string(), v.trim().trim_matches('"').to_string()))
        })
        .collect()
}

/// The language (LANG) and the formats' locale (LC_TIME, else LANG).
pub fn current() -> (String, String) {
    let conf = locale_conf();
    let get = |k: &str| conf.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
    let lang = get("LANG").unwrap_or_else(|| "en_US.UTF-8".into());
    let formats = get("LC_TIME").unwrap_or_else(|| lang.clone());
    (lang, formats)
}

/// The shell script that makes `r` and, once it exists, uses it for the formats (LANG kept).
/// None for names that could reach the shell as anything but a name.
pub fn script(r: &Region, lang: &str) -> Option<String> {
    if !safe(&r.locale) || !safe(lang) {
        return None;
    }
    let (loc, charset) = r.line.split_once(' ')?;
    if loc != r.locale || charset != "UTF-8" {
        return None;
    }
    let pattern = loc.replace('.', "\\.");
    // `locale -a` writes "en_GB.utf8" for en_GB.UTF-8.
    let listed = loc.replace(".UTF-8", ".utf8");
    let lc: Vec<String> = KEYS.iter().map(|k| format!("{k}={loc}")).collect();
    Some(format!(
        "set -e; \
         if ! grep -qE '^{pattern} UTF-8' /etc/locale.gen; then \
           if grep -qE '^#[[:space:]]*{pattern} UTF-8' /etc/locale.gen; then \
             sed -i -E 's/^#[[:space:]]*({pattern} UTF-8)/\\1/' /etc/locale.gen; \
           else echo '{loc} UTF-8' >> /etc/locale.gen; fi; \
           locale-gen; fi; \
         locale -a | grep -qix '{listed}'; \
         localectl set-locale LANG={lang} {}",
        lc.join(" ")
    ))
}

/// Uses `r` for the formats (asks for the administrator password once). Apps read it at the
/// next login.
pub async fn set(r: Region) -> Result<(), String> {
    let (lang, _) = current();
    let script = script(&r, &lang).ok_or("That region cannot be set")?;
    let ok = tokio::process::Command::new("pkexec")
        .args(["/bin/sh", "-c", &script])
        .status()
        .await
        .is_ok_and(|s| s.success());
    if ok {
        Ok(())
    } else {
        Err("The region was not changed".into())
    }
}

/// Formats back to the language's own (only LANG in locale.conf).
pub async fn reset() -> Result<(), String> {
    let (lang, _) = current();
    if !safe(&lang) {
        return Err("The language in locale.conf is not a locale name".into());
    }
    let ok = tokio::process::Command::new("pkexec")
        .args(["localectl", "set-locale", &format!("LANG={lang}")])
        .status()
        .await
        .is_ok_and(|s| s.success());
    if ok {
        Ok(())
    } else {
        Err("The region was not changed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_only_for_locale_names() {
        let r = Region {
            locale: "en_GB.UTF-8".into(),
            line: "en_GB.UTF-8 UTF-8".into(),
            name: "English (United Kingdom)".into(),
        };
        let s = script(&r, "en_US.UTF-8").unwrap();
        assert!(s.contains("LC_TIME=en_GB.UTF-8"));
        assert!(s.contains("grep -qix 'en_GB.utf8'"));
        assert!(s.contains("LANG=en_US.UTF-8"));
        let bad = Region {
            locale: "en_GB; rm -rf /".into(),
            line: "en_GB; rm -rf / UTF-8".into(),
            name: String::new(),
        };
        assert!(script(&bad, "en_US.UTF-8").is_none());
    }

    #[test]
    fn script_parses_as_shell() {
        for (loc, line) in [
            ("en_IN", "en_IN UTF-8"),
            ("en_GB.UTF-8", "en_GB.UTF-8 UTF-8"),
        ] {
            let r = Region {
                locale: loc.into(),
                line: line.into(),
                name: String::new(),
            };
            let s = script(&r, "en_US.UTF-8").unwrap();
            // -n: read and check only, nothing runs.
            let ok = std::process::Command::new("sh")
                .args(["-n", "-c", &s])
                .status()
                .is_ok_and(|st| st.success());
            assert!(ok, "{s}");
        }
    }

    #[test]
    fn names_from_glibc() {
        if std::path::Path::new("/usr/share/i18n/locales/en_NZ").exists() {
            assert_eq!(name_of("en_NZ"), "English (New Zealand)");
        }
    }
}
