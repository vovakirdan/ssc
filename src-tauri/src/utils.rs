use crate::peer::types::ServerConfig;
use rand::Rng;

pub fn random_id() -> String {
    hex::encode(rand::rng().random::<[u8; 16]>())
}

// Функция для добавления схемы протокола к URL ICE сервера, если она отсутствует
pub fn add_ice_url_scheme(config: &ServerConfig) -> String {
    // Если url уже начинается с "turn:" или "stun:", возвращаем как есть
    if ["turn:", "turns:", "stun:", "stuns:"]
        .iter()
        .any(|scheme| config.url.starts_with(scheme))
    {
        config.url.clone()
    } else {
        // В зависимости от типа сервера добавляем нужную схему
        let scheme = if config.r#type == "turn" {
            "turn:"
        } else {
            "stun:"
        };
        format!("{}{}", scheme, config.url)
    }
}
