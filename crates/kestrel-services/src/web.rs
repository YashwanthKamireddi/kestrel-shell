//! Weather (Open-Meteo, no account). It is
//! polled, so both poll slowly: weather every 30 minutes, scores every 30 seconds only while a
//! game is live.

use futures::{SinkExt, Stream};
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;

fn client() -> &'static reqwest::Client {
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Weather {
    pub place: String,
    pub temp: f32,
    pub code: u32,
    pub is_day: bool,
    pub high: f32,
    pub low: f32,
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
         &current=temperature_2m,weather_code,is_day\
         &daily=temperature_2m_max,temperature_2m_min&forecast_days=1&timezone=auto"
    ))
    .await?;
    let c = v.get("current")?;
    let d = v.get("daily")?;
    Some(Weather {
        place: place.to_string(),
        temp: c.get("temperature_2m")?.as_f64()? as f32,
        code: c.get("weather_code")?.as_u64()? as u32,
        is_day: c.get("is_day").and_then(Value::as_u64) == Some(1),
        high: d.get("temperature_2m_max")?.get(0)?.as_f64()? as f32,
        low: d.get("temperature_2m_min")?.get(0)?.as_f64()? as f32,
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
