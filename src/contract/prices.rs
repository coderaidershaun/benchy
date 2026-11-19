//! Token prices per model and cache class, read from `validation/prices.toml`, that turn telemetry tokens into cost.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::telemetry::ModelTokens;

#[derive(Debug, thiserror::Error)]
pub enum PricesError {
    #[error("failed to read prices {path}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse prices")]
    Parse(#[from] toml::de::Error),
    #[error("model {model} has no price; add it to validation/prices.toml")]
    UnknownModel { model: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prices {
    pub version: String,
    pub status: String,
    pub currency: String,
    /// Every price is per this many tokens.
    pub per_tokens: u64,
    #[serde(rename = "model")]
    pub models: Vec<ModelPrice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPrice {
    pub id: String,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
}

impl Prices {
    /// # Errors
    ///
    /// Fails when the file cannot be read or does not parse.
    pub fn load(path: &Path) -> Result<Prices, PricesError> {
        let text = std::fs::read_to_string(path).map_err(|source| PricesError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Ok(toml::from_str(&text)?)
    }

    /// Cost of one model's tokens. Thinking tokens are billed as output.
    ///
    /// # Errors
    ///
    /// Fails when the model has no price, so a new model is never costed at zero.
    pub fn cost_usd(&self, tokens: &ModelTokens) -> Result<f64, PricesError> {
        let price = self
            .models
            .iter()
            .find(|m| m.id == tokens.model)
            .ok_or_else(|| PricesError::UnknownModel {
                model: tokens.model.clone(),
            })?;
        let priced = [
            (tokens.input, price.input),
            (tokens.output + tokens.thinking, price.output),
            (tokens.cache_read, price.cache_read),
            (tokens.cache_write_5m, price.cache_write_5m),
            (tokens.cache_write_1h, price.cache_write_1h),
        ];
        let per_tokens = self.per_tokens as f64;
        Ok(priced
            .iter()
            .map(|&(count, usd)| count as f64 * usd / per_tokens)
            .sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_prices() -> Prices {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("validation/prices.toml");
        Prices::load(&path).expect("validation/prices.toml is valid")
    }

    #[test]
    fn a_million_output_tokens_cost_the_output_price() {
        let prices = repo_prices();
        let tokens = ModelTokens {
            model: "claude-opus-5-5".to_string(),
            output: 600_000,
            thinking: 400_000,
            ..ModelTokens::default()
        };
        let output_price = prices
            .models
            .iter()
            .find(|m| m.id == tokens.model)
            .expect("the model is priced")
            .output;
        assert_eq!(prices.cost_usd(&tokens).expect("priced"), output_price);
    }

    #[test]
    fn an_unknown_model_is_an_error() {
        let tokens = ModelTokens {
            model: "claude-unknown".to_string(),
            ..ModelTokens::default()
        };
        assert!(matches!(
            repo_prices().cost_usd(&tokens),
            Err(PricesError::UnknownModel { .. })
        ));
    }
}
