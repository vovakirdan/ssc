// Конфигурация приложения
// Логирование можно отключить только в режиме разработки

#[cfg(debug_assertions)]
pub const LOGGING_ENABLED: bool = true; // В режиме отладки логирование включено

#[cfg(not(debug_assertions))]
pub const LOGGING_ENABLED: bool = false; // В продакшене логирование отключено

// Дополнительные настройки для режима разработки
#[cfg(debug_assertions)]
pub mod dev {
    // Для полного отключения логирования в режиме разработки
    // измените эту константу на false
    // ВАЖНО: Эта настройка работает только в debug режиме!
    pub const ENABLE_LOGGING: bool = true;
}

#[cfg(not(debug_assertions))]
pub mod dev {
    // В продакшене все дополнительные настройки отключены
    // Эти константы не могут быть изменены в продакшене
    pub const VERBOSE_LOGGING: bool = false;
    pub const ENABLE_LOGGING: bool = false;
} 