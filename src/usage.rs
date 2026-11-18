use crate::config::AppConfig;
use crate::provider::ProviderKind;

pub use crate::azure::MONTHLY_CHAR_LIMIT as AZURE_MONTHLY_LIMIT;

pub fn deepl_remaining(config: &AppConfig) -> u64 {
    config
        .deepl_character_limit
        .saturating_sub(config.deepl_character_count)
}

pub fn azure_remaining(config: &AppConfig, month: &str) -> u64 {
    let used = if config.azure_usage_month == month {
        config.azure_usage_chars
    } else {
        0
    };
    AZURE_MONTHLY_LIMIT.saturating_sub(used)
}

pub fn set_deepl_usage(config: &mut AppConfig, count: u64, limit: u64) {
    config.deepl_usage_known = true;
    config.deepl_character_count = count;
    config.deepl_character_limit = if limit == 0 { 1_000_000 } else { limit };
}

/// Add characters billed by a successful call. Azure's counter resets when
/// `month` (YYYY-MM) changes because the free tier is monthly.
pub fn record_usage(config: &mut AppConfig, provider: ProviderKind, chars: u64, month: &str) {
    match provider {
        ProviderKind::Deepl => {
            if !config.deepl_usage_known {
                config.deepl_usage_known = true;
                if config.deepl_character_limit == 0 {
                    config.deepl_character_limit = 1_000_000;
                }
            }
            config.deepl_character_count = config.deepl_character_count.saturating_add(chars);
        }
        ProviderKind::Azure => {
            if config.azure_usage_month != month {
                config.azure_usage_month = month.to_string();
                config.azure_usage_chars = 0;
            }
            config.azure_usage_chars = config.azure_usage_chars.saturating_add(chars);
            config.azure_connected = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mockup_remaining_counts() {
        let mut config = AppConfig::default();
        set_deepl_usage(&mut config, 132_580, 1_000_000);
        assert_eq!(deepl_remaining(&config), 867_420);

        config.azure_usage_month = "2026-11".into();
        config.azure_usage_chars = 246_000;
        assert_eq!(azure_remaining(&config, "2026-11"), 1_754_000);
        assert_eq!(azure_remaining(&config, "2026-12"), AZURE_MONTHLY_LIMIT);
    }

    #[test]
    fn azure_month_rolls_over_before_adding() {
        let mut config = AppConfig::default();
        config.azure_usage_month = "2026-10".into();
        config.azure_usage_chars = 246_000;
        record_usage(&mut config, ProviderKind::Azure, 20, "2026-11");
        assert_eq!(config.azure_usage_month, "2026-11");
        assert_eq!(config.azure_usage_chars, 20);
        assert!(config.azure_connected);
        assert_eq!(
            azure_remaining(&config, "2026-11"),
            AZURE_MONTHLY_LIMIT - 20
        );
    }

    #[test]
    fn deepl_usage_accumulates_after_the_first_call() {
        let mut config = AppConfig::default();
        record_usage(&mut config, ProviderKind::Deepl, 12, "2026-11");
        assert!(config.deepl_usage_known);
        assert_eq!(config.deepl_character_count, 12);
        record_usage(&mut config, ProviderKind::Deepl, 8, "2026-11");
        assert_eq!(config.deepl_character_count, 20);
    }
}
