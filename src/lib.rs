use std::collections::HashSet;
use std::{collections::HashMap};
use std::str::FromStr;
use urlencoding::decode;

use bitcoin::{Address, Amount, address::NetworkUnchecked};

#[derive(Debug, PartialEq, Eq)]
pub struct Bip321 {
    address: Option<Address<NetworkUnchecked>>,
    amount: Option<Amount>,
    label: Option<String>,
    message: Option<String>,

    pop: Option<String>,
    req_pop: Option<String>,
    extra: HashMap<String, String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Bip321Errors {
    DuplicateParams,
    IncorrectSchema,
    InvalidAddress,
    InvalidAmount
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
            pop: None,
            req_pop: None,
            extra: HashMap::new(),
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
        let mut result = Bip321::default();

        let (address, queryparams) = match body.find("?") {
            Some(pos) => {
                (&body[..pos], &body[pos + 1..])
            },
            None => (&body[..], "")
        };

        let mut seens = HashSet::new();

        for param in queryparams.split("&") {
            if let Some((key, value)) = param.split_once("=") {
                let key_lower = key.to_lowercase();               
                match key_lower.as_str() {
                    "label" | "message" | "pop" if seens.contains(key_lower.as_str()) => {
                        return Err(Bip321Errors::DuplicateParams);
                    }
                    "amount" => {
                        let value: f64 = value.trim().parse().unwrap();
                        let amount = Amount::from_btc(value).map_err(|_| Bip321Errors::InvalidAmount)?;
                        result.amount = Some(amount);
                    },
                    "label" => {
                        let label = decode(value).expect("UFT-8").into_owned();
                        result.label = Some(label);
                        seens.insert(key_lower);
                    },
                    "message" => {
                        let message = decode(value).expect("UFT-8").into_owned();
                        result.message = Some(message);
                        seens.insert(key_lower);
                    },
                    "pop" => {
                        let pop = value.to_string();
                        result.pop = Some(pop);
                        seens.insert(key_lower);
                    }
                    _ => {
                        result.extra.insert(key_lower, value.to_string());
                    }
                }
            }
        }

        result.address = if address.is_empty() {
            None
        } else {
            Some(address.parse().map_err(|_| Bip321Errors::InvalidAddress)?)
        };

        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn url() {
        let url = " bitcoin:?amount=50&label=Luke-Jr&message=Donation%20for%20project%20xyz";
        let bip321result = url.parse_url_to_bip321().unwrap();
        println!("{:#?}", bip321result);

        assert!(
        bip321result.address.unwrap() == Address::from_str("").unwrap()
    );
    }
}
