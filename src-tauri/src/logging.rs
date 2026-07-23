use std::{fmt, sync::OnceLock};

use tracing::Level;

static INITIALIZED: OnceLock<()> = OnceLock::new();

pub fn initialize() {
    INITIALIZED.get_or_init(|| {
        let _ = tracing_subscriber::fmt()
            .with_max_level(Level::INFO)
            .with_target(false)
            .without_time()
            .try_init();
    });
}

pub struct Sensitive<T>(pub T);

impl<T> fmt::Debug for Sensitive<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl<T> fmt::Display for Sensitive<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[cfg(test)]
mod tests {
    use super::Sensitive;

    #[test]
    fn 敏感值不會出現在格式化輸出() {
        let secret = Sensitive("私人視窗標題");
        assert_eq!(format!("{secret:?}"), "[REDACTED]");
        assert_eq!(format!("{secret}"), "[REDACTED]");
    }
}
