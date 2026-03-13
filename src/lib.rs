use bitcoin::Denomination;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use urlencoding::decode;

use bitcoin::{Address, Amount, address::NetworkUnchecked};

#[derive(Debug, PartialEq, Eq)]
pub struct Bip321 {
    pub address: Option<Address<NetworkUnchecked>>,
    pub amount: Option<Amount>,
    pub label: Option<String>,
    pub message: Option<String>,
    pub pop: Option<String>,
    pub pop_required: bool,
    pub instructions: HashMap<String, Vec<String>>,
}

#[derive(Debug)]
pub struct NoExtra {}

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

trait Bip321parser {
    fn parse_url_to_bip321(&self) -> Result<Bip321, Bip321Errors>;
}

impl Bip321parser for str {
    fn parse_url_to_bip321(&self) -> Result<Bip321, Bip321Errors> {
        self.parse::<Bip321>()
    }
}

impl Default for Bip321 {
    fn default() -> Self {
        Bip321 {
            address: None,
            amount: None,
            label: None,
            message: None,
            pop_required: false,
            pop: None,
            instructions: HashMap::new(),
        }
    }
}

impl FromStr for Bip321 {
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
        let mut result = Bip321::default();

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
                        result.label = Some(decode(value).expect("UFT-8").into_owned());
                    }
                    "message" => {
                        result.message = Some(decode(value).expect("UFT-8").into_owned());
                    }
                    "pop" | "req-pop" => {
                        let forbidden_schemes = ["http", "https", "file", "javascript", "mailto"];
                        if seens.contains("pop") {
                            return Err(Bip321Errors::DuplicateParams);
                        }
                        if key_lower == "req-pop" {
                            result.pop_required = true;
                        }
                        let value_lower = value.to_lowercase();

                        if forbidden_schemes
                            .iter()
                            .any(|&scheme| value_lower.starts_with(scheme))
                        {
                            return Err(Bip321Errors::IncorrectSchema);
                        }
                        let decoded_val = decode(value)
                            .map(|s| s.into_owned())
                            .map_err(|_| Bip321Errors::InvalidEncoding)?;

                        result.pop = Some(decoded_val);
                    }
                    _ => {
                        if key_lower.starts_with("req-") {
                            let (_req, key) = key.split_once("-").unwrap_or((&key_lower, ""));
                            if !result.instructions.contains_key(key) {
                                return Err(Bip321Errors::InvalidRequiredPayment);
                            }
                        }
                        result.instructions.entry(key_lower).or_default().push(
                            decode(value)
                                .ok()
                                .map(|s| s.into_owned())
                                .unwrap_or_default(),
                        );
                    }
                }
            }
        }
        if result.address.is_none() && result.instructions.is_empty() {
            return Err(Bip321Errors::NoOnePaymentWasFound);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn url() {
        let url = "bitcoin:?lightning=lnbc420bogusinvoice";
        let bip321result = Bip321parser::parse_url_to_bip321(url);
        println!("{:#?}", bip321result);
    }
}
