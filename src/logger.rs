use log::{LevelFilter, Log, Metadata, Record};

/// No timestamps: procd and Docker add their own.
struct StderrLogger;

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

pub fn init(level: LevelFilter) {
    log::set_logger(&StderrLogger).expect("logger is initialized once");
    log::set_max_level(level);
}
