use bitcoin::Denomination;
use bitcoin::{Address, Amount, address::NetworkUnchecked};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use urlencoding::decode;

#[derive(Debug, PartialEq, Eq)]
pub struct Bip321<T: Bip321ExtraHandle> {
    pub address: Option<Address<NetworkUnchecked>>,
    pub amount: Option<Amount>,
    pub label: Option<String>,
    pub message: Option<String>,
    pub pop: Option<String>,
    pub pop_required: bool,
    pub extras: Option<T>,
}

pub trait Bip321ExtraHandle: Default {
    fn handle_param(&mut self, key: &str, value: Vec<String>) -> Result<(), Bip321Errors>;

    fn is_empty(&self) -> bool;

    fn is_supported_key(&self, key: &str) -> bool;
}

#[derive(Debug, PartialEq, Eq)]
pub enum Bip321Errors {
    DuplicateParams,
    IncorrectSchema,
    InvalidAddress,
    InvalidAmount,
    NoOnePaymentWasFound,
    InvalidEncoding,
    InvalidRequiredPayment,
}

impl<T: Bip321ExtraHandle> Default for Bip321<T> {
    fn default() -> Self {
        Bip321 {
            address: None,
            amount: None,
            label: None,
            message: None,
            pop_required: false,
            pop: None,
            extras: None,
        }
    }
}

impl<T: Bip321ExtraHandle> FromStr for Bip321<T> {
    type Err = Bip321Errors;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let uri = s.trim();

        if !uri.to_lowercase().starts_with("bitcoin:") {
            return Err(Bip321Errors::IncorrectSchema);
        }

        let body = &uri[8..];
        let (address_str, query_str) = match body.find("?") {
            Some(pos) => (&body[..pos], &body[pos + 1..]),
            None => (&body[..], ""),
        };

        let mut seens = HashSet::new();
        let mut extra_params: HashMap<String, Vec<String>> = HashMap::new();
        let mut result: Bip321<T> = Bip321::default();

        if !address_str.is_empty() {
            let addr = address_str
                .parse::<Address<NetworkUnchecked>>()
                .map_err(|_| Bip321Errors::InvalidAddress)?;

            result.address = Some(addr);
        }

        if !query_str.is_empty() {
            for param in query_str.split("&") {
                let (key, value) = param.split_once("=").unwrap_or((param, ""));
                let key_lower = key.to_lowercase();

                let check_key = if key_lower != "req-pop" {
                    key_lower.clone()
                } else {
                    "pop".to_string()
                };

                match check_key.as_str() {
                    "amount" | "label" | "message" | "pop" => {
                        if !seens.insert(check_key.clone()) {
                            return Err(Bip321Errors::DuplicateParams);
                        }
                    }
                    _ => {}
                }

                match key_lower.as_str() {
                    "amount" => {
                        if value.contains(",") {
                            return Err(Bip321Errors::InvalidAmount);
                        }
                        let amt = Amount::from_str_in(value, Denomination::Bitcoin)
                            .map_err(|_| Bip321Errors::InvalidAmount)?;
                        result.amount = Some(amt);
                    }
                    "label" => {
                        result.label = Some(
                            decode(value)
                                .map_err(|_| Bip321Errors::InvalidEncoding)?
                                .into_owned(),
                        );
                    }
                    "message" => {
                        result.message = Some(
                            decode(value)
                                .map_err(|_| Bip321Errors::InvalidEncoding)?
                                .into_owned(),
                        );
                    }
                    "pop" | "req-pop" => {
                        let forbidden_schemes = ["http", "https", "file", "javascript", "mailto"];
                        let decoded_val = decode(value)
                            .map(|s| s.into_owned())
                            .map_err(|_| Bip321Errors::InvalidEncoding)?;
                        let value_lower = decoded_val.to_lowercase();

                        if forbidden_schemes
                            .iter()
                            .any(|&s| value_lower.starts_with(s))
                        {
                            if key_lower == "req-pop" {
                                return Err(Bip321Errors::IncorrectSchema);
                            }
                            result.pop = None;
                        } else {
                            result.pop = Some(decoded_val);
                            if key_lower == "req-pop" {
                                result.pop_required = true;
                            }
                        }
                    }
                    _ => {
                        let decoded_val = decode(value)
                            .map(|s| s.into_owned())
                            .map_err(|_| Bip321Errors::InvalidEncoding)?;
                        extra_params
                            .entry(key_lower.clone())
                            .or_insert(Vec::new())
                            .push(decoded_val);
                    }
                }
            }
        }

        for (key, values) in extra_params {
            let ext = result.extras.get_or_insert_with(T::default);
            if key.starts_with("req-") {
                let stripped = &key[4..];
                if !ext.is_supported_key(stripped) {
                    return Err(Bip321Errors::InvalidRequiredPayment);
                }
                ext.handle_param(stripped, values)?;
            } else {
                ext.handle_param(&key, values)?
            }
        }

        if result.address.is_none() && result.extras.is_none() {
            return Err(Bip321Errors::NoOnePaymentWasFound);
        }

        Ok(result)
    }
}

#[derive(Debug, Default)]
pub struct MyExtras {
    pj: Vec<String>,
}

impl Bip321ExtraHandle for MyExtras {
    fn handle_param(&mut self, key: &str, value: Vec<String>) -> Result<(), Bip321Errors> {
        match key {
            "pj" => {
                self.pj.extend(value);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn is_empty(&self) -> bool {
        self.pj.is_empty()
    }

    fn is_supported_key(&self, key: &str) -> bool {
        matches!(key, "pj")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn url() {
        let url = "bitcoin:?pj=https://endpoint1.com&pj=https://endpoint2.com&bc=bc1q...&bc=bc1p";
        let bip321result = url.parse::<Bip321<MyExtras>>();
        println!("{:#?}", bip321result);
    }
}
