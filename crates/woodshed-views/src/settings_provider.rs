//! Woodshed's adapter for the shared settings projection contract.
//!
//! `AppSettings` remains the product-owned typed model. This adapter gives the
//! Settings views a provider-shaped description and typed write path without
//! moving persistence into the host contract.

use genet_host_api::settings::{
    SettingControl, SettingMovement, SettingMutability, SettingOption, SettingScope,
    SettingSecurity, SettingSpec, SettingValue, SettingsError, SettingsProvider,
};
use genet_host_api::tile::SettingsRef;
use woodshed_core::settings::AppSettings;

use crate::theme::ThemeMode;

pub const APPEARANCE_REFERENCE: &str = "pelt/appearance";
pub const ACCESSIBILITY_REFERENCE: &str = "pelt/accessibility";

/// A provider over one Woodshed `AppSettings` value.
#[derive(Clone, Debug)]
pub struct WoodshedSettingsProvider {
    settings: AppSettings,
}

impl WoodshedSettingsProvider {
    pub fn new(settings: AppSettings) -> Self {
        Self { settings }
    }

    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    pub fn into_settings(self) -> AppSettings {
        self.settings
    }
}

fn theme_options() -> Vec<SettingOption> {
    ThemeMode::ALL
        .into_iter()
        .map(|mode| SettingOption {
            value: mode.label().into(),
            label: mode.label().into(),
        })
        .collect()
}

fn text_scale_options() -> Vec<SettingOption> {
    ["Normal", "Large", "Larger"]
        .into_iter()
        .map(|value| SettingOption {
            value: value.into(),
            label: value.into(),
        })
        .collect()
}

fn ordinary_application(
    control: SettingControl,
    id: &str,
    label: &str,
    value: SettingValue,
) -> SettingSpec {
    SettingSpec {
        id: id.into(),
        label: label.into(),
        scope: SettingScope::Application,
        movement: SettingMovement::LocalOnly,
        mutability: SettingMutability::Live,
        security: SettingSecurity::Ordinary,
        control,
        value,
    }
}

impl SettingsProvider for WoodshedSettingsProvider {
    fn describe(&self, reference: &SettingsRef) -> Result<Vec<SettingSpec>, SettingsError> {
        match reference.0.as_str() {
            APPEARANCE_REFERENCE => Ok(vec![ordinary_application(
                SettingControl::Choice {
                    options: theme_options(),
                },
                "appearance.theme",
                "Theme",
                SettingValue::Text(self.settings.appearance.theme.clone()),
            )]),
            ACCESSIBILITY_REFERENCE => Ok(vec![
                ordinary_application(
                    SettingControl::Toggle,
                    "accessibility.reduce_motion",
                    "Reduce motion",
                    SettingValue::Boolean(self.settings.accessibility.reduce_motion),
                ),
                ordinary_application(
                    SettingControl::Toggle,
                    "accessibility.distinguish_root",
                    "Distinguish root by outline",
                    SettingValue::Boolean(self.settings.accessibility.distinguish_root),
                ),
                ordinary_application(
                    SettingControl::Choice {
                        options: text_scale_options(),
                    },
                    "accessibility.text_scale",
                    "Text size",
                    SettingValue::Text(self.settings.accessibility.text_scale.clone()),
                ),
            ]),
            _ => Err(SettingsError::UnsupportedReference(reference.clone())),
        }
    }

    fn apply(
        &mut self,
        reference: &SettingsRef,
        setting_id: &str,
        value: SettingValue,
    ) -> Result<(), SettingsError> {
        if reference.0 == APPEARANCE_REFERENCE && setting_id == "appearance.theme" {
            let SettingValue::Text(theme) = value else {
                return Err(SettingsError::InvalidValue {
                    setting_id: setting_id.into(),
                    message: "expected Text".into(),
                });
            };
            if ThemeMode::from_name(&theme).is_none() {
                return Err(SettingsError::InvalidValue {
                    setting_id: setting_id.into(),
                    message: format!("unknown theme {theme}"),
                });
            }
            self.settings.appearance.theme = theme;
            return Ok(());
        }

        if reference.0 == ACCESSIBILITY_REFERENCE {
            match (setting_id, value) {
                ("accessibility.reduce_motion", SettingValue::Boolean(value)) => {
                    self.settings.accessibility.reduce_motion = value;
                    return Ok(());
                }
                ("accessibility.distinguish_root", SettingValue::Boolean(value)) => {
                    self.settings.accessibility.distinguish_root = value;
                    return Ok(());
                }
                ("accessibility.text_scale", SettingValue::Text(value))
                    if text_scale_options()
                        .iter()
                        .any(|option| option.value == value) =>
                {
                    self.settings.accessibility.text_scale = value;
                    return Ok(());
                }
                (_, value) => {
                    return Err(SettingsError::InvalidValue {
                        setting_id: setting_id.into(),
                        message: format!("unsupported value {value:?}"),
                    });
                }
            }
        }

        if reference.0 != APPEARANCE_REFERENCE && reference.0 != ACCESSIBILITY_REFERENCE {
            return Err(SettingsError::UnsupportedReference(reference.clone()));
        }
        Err(SettingsError::UnknownSetting(setting_id.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_theme_and_accessibility_with_axes() {
        let provider = WoodshedSettingsProvider::new(AppSettings::default());
        let appearance = provider
            .describe(&SettingsRef(APPEARANCE_REFERENCE.into()))
            .unwrap();
        assert_eq!(
            appearance[0].control,
            SettingControl::Choice {
                options: theme_options(),
            }
        );
        assert_eq!(appearance[0].scope, SettingScope::Application);
        assert_eq!(appearance[0].movement, SettingMovement::LocalOnly);

        let accessibility = provider
            .describe(&SettingsRef(ACCESSIBILITY_REFERENCE.into()))
            .unwrap();
        assert_eq!(accessibility.len(), 3);
    }

    #[test]
    fn applies_typed_writes_to_product_settings() {
        let mut provider = WoodshedSettingsProvider::new(AppSettings::default());
        let reference = SettingsRef(APPEARANCE_REFERENCE.into());
        provider
            .apply(
                &reference,
                "appearance.theme",
                SettingValue::Text("Ember".into()),
            )
            .unwrap();
        assert_eq!(provider.settings().appearance.theme, "Ember");
        assert!(matches!(
            provider.apply(&reference, "appearance.theme", SettingValue::Boolean(true)),
            Err(SettingsError::InvalidValue { .. })
        ));
    }
}
