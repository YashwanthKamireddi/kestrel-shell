//! Weather (Open-Meteo, no account). It is
//! polled, so both poll slowly: weather every 30 minutes, scores every 30 seconds only while a
//! game is live.

use futures::{SinkExt, Stream};
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;

/// kestrel's one HTTP client (it names itself: the AUR's bot shield answers a nameless
/// request with a challenge page).
pub(crate) fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("kestrel/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default()
    })
}

async fn get(url: &str) -> Option<Value> {
    client().get(url).send().await.ok()?.json().await.ok()
}

// --- Weather -------------------------------------------------------------------------------

/// Where the last weather is kept, so the bar shows it on its first frame after a start (it
/// popped in seconds later, when the network answered, and pushed the bar's items aside).
fn weather_file() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".cache")))?;
    Some(base.join("kestrel").join("weather.json"))
}

/// The last weather read, if any.
pub fn cached_weather() -> Option<Weather> {
    serde_json::from_slice(&std::fs::read(weather_file()?).ok()?).ok()
}

/// Keeps `w` for the next start (written off the caller's thread).
pub fn keep_weather(w: &Weather) {
    let (Some(f), Ok(bytes)) = (weather_file(), serde_json::to_vec(w)) else {
        return;
    };
    std::thread::spawn(move || {
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = f.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &f);
        }
    });
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Weather {
    pub place: String,
    pub temp: f32,
    pub code: u32,
    pub is_day: bool,
    pub high: f32,
    pub low: f32,
    /// Feels like (°C), relative humidity (%), wind (km/h).
    pub feels: f32,
    pub humidity: f32,
    pub wind: f32,
    /// Today's sunrise and sunset, "HH:MM".
    pub sunrise: String,
    pub sunset: String,
    /// The next hours: (hour, temperature, weather code, day or night).
    pub hours: Vec<(u32, f32, u32, bool)>,
    /// The coming days: (date "YYYY-MM-DD", weather code, high, low).
    pub days: Vec<(String, u32, f32, f32)>,
}

impl Weather {
    pub fn describe(&self) -> &'static str {
        match self.code {
            0 => "Clear",
            1 => "Mostly clear",
            2 => "Partly cloudy",
            3 => "Overcast",
            45 | 48 => "Fog",
            51..=57 => "Drizzle",
            61..=67 | 80..=82 => "Rain",
            71..=77 | 85 | 86 => "Snow",
            95..=99 => "Thunderstorm",
            _ => "",
        }
    }
}

async fn locate(city: &str) -> Option<(f64, f64, String)> {
    let q: String = city
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    let v = get(&format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={q}&count=1"
    ))
    .await?;
    let r = v.get("results")?.get(0)?;
    Some((
        r.get("latitude")?.as_f64()?,
        r.get("longitude")?.as_f64()?,
        r.get("name")?.as_str()?.to_string(),
    ))
}

async fn weather_once(lat: f64, lon: f64, place: &str) -> Option<Weather> {
    let v = get(&format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat:.3}&longitude={lon:.3}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,\
         weather_code,is_day\
         &hourly=temperature_2m,weather_code,is_day&forecast_hours=12\
         &daily=temperature_2m_max,temperature_2m_min,weather_code,sunrise,sunset\
         &forecast_days=6&timezone=auto"
    ))
    .await?;
    let c = v.get("current")?;
    let d = v.get("daily")?;
    let h = v.get("hourly");
    let f = |o: &Value, k: &str| o.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let list = |o: &Value, k: &str| {
        o.get(k)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let hhmm = |s: &Value| {
        s.as_str()
            .and_then(|s| s.get(11..16))
            .unwrap_or("")
            .to_string()
    };
    let hours = h
        .map(|h| {
            let (time, temp, code, day) = (
                list(h, "time"),
                list(h, "temperature_2m"),
                list(h, "weather_code"),
                list(h, "is_day"),
            );
            time.iter()
                .enumerate()
                .filter_map(|(i, t)| {
                    let hour = t.as_str()?.get(11..13)?.parse().ok()?;
                    Some((
                        hour,
                        temp.get(i)?.as_f64()? as f32,
                        code.get(i)?.as_u64()? as u32,
                        day.get(i).and_then(Value::as_u64) == Some(1),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let (dt, dc, dmax, dmin) = (
        list(d, "time"),
        list(d, "weather_code"),
        list(d, "temperature_2m_max"),
        list(d, "temperature_2m_min"),
    );
    let days = dt
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            Some((
                t.as_str()?.to_string(),
                dc.get(i)?.as_u64()? as u32,
                dmax.get(i)?.as_f64()? as f32,
                dmin.get(i)?.as_f64()? as f32,
            ))
        })
        .collect();
    Some(Weather {
        place: place.to_string(),
        temp: c.get("temperature_2m")?.as_f64()? as f32,
        code: c.get("weather_code")?.as_u64()? as u32,
        is_day: c.get("is_day").and_then(Value::as_u64) == Some(1),
        high: d.get("temperature_2m_max")?.get(0)?.as_f64()? as f32,
        low: d.get("temperature_2m_min")?.get(0)?.as_f64()? as f32,
        feels: f(c, "apparent_temperature"),
        humidity: f(c, "relative_humidity_2m"),
        wind: f(c, "wind_speed_10m"),
        sunrise: list(d, "sunrise").first().map(hhmm).unwrap_or_default(),
        sunset: list(d, "sunset").first().map(hhmm).unwrap_or_default(),
        hours,
        days,
    })
}

/// Where to get the weather for: a city to look up, or exact coordinates with a label.
#[derive(Clone, Debug, Hash)]
pub struct Place {
    pub name: String,
    /// Latitude and longitude as fixed-point (micro-degrees), so the place can be hashed.
    pub coords: Option<(i64, i64)>,
}

/// Weather for a place, refreshed every 30 minutes.
pub fn weather_stream(p: &Place) -> impl Stream<Item = Weather> + use<> {
    let city = p.name.clone();
    let fixed = p
        .coords
        .map(|(a, b)| (a as f64 / 1e6, b as f64 / 1e6, p.name.clone()));
    crate::stream::channel(1, async move |mut out| {
        let mut place = fixed;
        loop {
            if place.is_none() {
                place = locate(&city).await;
            }
            if let Some((lat, lon, name)) = &place
                && let Some(w) = weather_once(*lat, *lon, name).await
                && out.send(w).await.is_err()
            {
                return;
            }
            let wait = if place.is_some() { 30 * 60 } else { 5 * 60 };
            tokio::time::sleep(Duration::from_secs(wait)).await;
        }
    })
}
