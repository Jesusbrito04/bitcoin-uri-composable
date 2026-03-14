use bitcoin::Denomination;
use std::collections::HashSet;
use std::str::FromStr;
use urlencoding::decode;

use bitcoin::{Address, Amount, address::NetworkUnchecked};

#[derive(Debug, PartialEq, Eq)]
pub struct Bip321<T: Bip321ExtraHandle> {
    pub address: Option<Address<NetworkUnchecked>>,
    pub amount: Option<Amount>,
    pub label: Option<String>,
    pub message: Option<String>,
    pub pop: Option<String>,
    pub pop_required: bool,
    pub extras: T,
}

#[derive(Debug, Default)]
pub struct MyExtras {
    pj: String,
}

impl Bip321ExtraHandle for MyExtras {
    fn handle_param(&mut self, key: &str, value: String) -> Result<(), Bip321Errors> {
        match key {
            "pj" => {
                self.pj = value;
                Ok(())
            }
            k if key.starts_with("req-") => {
                let stripped_key = &k[4..];
                if !self.support_key(stripped_key) {
                    return Err(Bip321Errors::InvalidRequiredPayment);
                }
                self.handle_param(stripped_key, value)
            }
            _ => Ok(()),
        }
    }

    fn is_empty(&self) -> bool {
        self.pj.is_empty()
    }

    fn support_key(&self, key: &str) -> bool {
        matches!(key, "pj")
    }
}

pub trait Bip321ExtraHandle: Default {
    fn handle_param(&mut self, key: &str, value: String) -> Result<(), Bip321Errors>;

    fn is_empty(&self) -> bool;

    fn support_key(&self, key: &str) -> bool; 
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
            extras: T::default(),
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
                                .map_err(|_| Bip321Errors::InvalidEncoding)
                                .unwrap()
                                .into_owned(),
                        );
                    }
                    "message" => {
                        result.message = Some(
                            decode(value)
                                .map_err(|_| Bip321Errors::InvalidEncoding)
                                .unwrap()
                                .into_owned(),
                        );
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
                        let decoded_val = decode(value)
                            .map(|s| s.into_owned())
                            .map_err(|_| Bip321Errors::InvalidEncoding)?;

                        result.extras.handle_param(&key_lower, decoded_val)?;
                    }
                }
            }
        }

        if result.address.is_none() && result.extras.is_empty() {
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
        let url = "bitcoin:?req-sp=sp1qsilentpayment";
        let bip321result = url.parse::<Bip321<MyExtras>>();
        println!("{:#?}", bip321result);
    }
}
