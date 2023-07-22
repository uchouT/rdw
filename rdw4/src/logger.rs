/// cbindgen:ignore
// from https://github.com/rust-lang/log/issues/421#issuecomment-990617341
#[cfg(not(feature = "bindings"))]
#[no_mangle]
pub fn setup_logger(
    logger: &'static dyn log::Log,
    level: log::LevelFilter,
) -> Result<(), log::SetLoggerError> {
    log::set_max_level(level);
    log::set_logger(logger)
}

#[cfg(feature = "bindings")]
extern "Rust" {
    pub fn setup_logger(
        logger: &'static dyn log::Log,
        level: log::LevelFilter,
    ) -> Result<(), log::SetLoggerError>;
}
