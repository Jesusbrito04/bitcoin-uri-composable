use bitcoin::{
    Address, Amount, Denomination, Network,
    address::{NetworkChecked, NetworkUnchecked, NetworkValidation},
};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use urlencoding::{decode, encode};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Bip321Errors<'a> {
    DuplicateParam(&'a str),
    IncorrectSchema,
    InvalidAddress(&'a str),
    InvalidAmount,
    NoOnePaymentWasFound,
    InvalidEncoding,
    InvalidRequiredPayment,
}


pub trait Bip321ExtraHandle<'a>
where
    Self: Default,
{
    fn handle_param(
        &mut self,
        key: &'a str,
        value: Vec<Cow<'a, str>>,
    ) -> Result<(), Bip321Errors<'a>>;

    fn validate(&self, _network: Network) -> Result<(), Bip321Errors<'a>> {
        Ok(())
    }

    fn is_empty(&self) -> bool;

    fn is_supported_key(&self, key: &str) -> bool;
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Bip321<'a, T, NetVal = NetworkUnchecked>
where
    T: Bip321ExtraHandle<'a>,
    NetVal: NetworkValidation,
{
    pub address: Option<Address<NetVal>>,
    pub amount: Option<Amount>,
    pub label: Option<Cow<'a, str>>,
    pub message: Option<Cow<'a, str>>,
    pub pop: Option<Cow<'a, str>>,
    pub extras: Option<T>,
}

impl<'a, T: Bip321ExtraHandle<'a>> Bip321<'a, T> {
    pub fn parse_url(s: &'a str) -> Result<Self, Bip321Errors<'a>> {
        let uri = s.trim();

        if uri.len() < 8 || !uri[..8].eq_ignore_ascii_case("bitcoin:") {
            return Err(Bip321Errors::IncorrectSchema);
        }

        let body = &uri[8..];
        let (address_str, query_str) = match body.find("?") {
            Some(pos) => (&body[..pos], &body[pos + 1..]),
            None => (&body[..], ""),
        };

        let mut seens: HashSet<&'a str> = HashSet::new();
        let mut extra_params: HashMap<&'a str, Vec<Cow<'a, str>>> = HashMap::new();
        let mut result: Bip321<T, NetworkUnchecked> = Bip321::default();

        if !address_str.is_empty() {
            let addr = address_str
                .parse::<Address<NetworkUnchecked>>()
                .map_err(|_| Bip321Errors::InvalidAddress(address_str))?;

            result.address = Some(addr);
        }

        if !query_str.is_empty() {
            for param in query_str.split("&") {
                let (key, value) = param.split_once("=").unwrap_or((param, ""));

                let is_pop_related =
                    key.eq_ignore_ascii_case("pop") || key.eq_ignore_ascii_case("req-pop");

                if is_pop_related {
                    if !seens.insert("pop") {
                        return Err(Bip321Errors::DuplicateParam(key));
                    }
                } else if matches!(key, "amount" | "label" | "message") {
                    if !seens.insert(key) {
                        return Err(Bip321Errors::DuplicateParam(key));
                    }
                }

                match key {
                    "amount" => {
                        if value.contains(",") {
                            return Err(Bip321Errors::InvalidAmount);
                        }
                        let amt = Amount::from_str_in(value, Denomination::Bitcoin)
                            .map_err(|_| Bip321Errors::InvalidAmount)?;
                        result.amount = Some(amt);
                    }
                    "label" => {
                        result.label =
                            Some(decode(value).map_err(|_| Bip321Errors::InvalidEncoding)?);
                    }
                    "message" => {
                        result.message =
                            Some(decode(value).map_err(|_| Bip321Errors::InvalidEncoding)?);
                    }
                    "pop" | "req-pop" => {
                        let forbidden_schemes = ["http", "https", "file", "javascript", "mailto"];
                        let decoded_val = decode(value)
                            .map(|s| s)
                            .map_err(|_| Bip321Errors::InvalidEncoding)?;
                        let value_lower = decoded_val.to_lowercase();

                        if forbidden_schemes
                            .iter()
                            .any(|&s| value_lower.starts_with(s))
                        {
                            return Err(Bip321Errors::IncorrectSchema);
                        } else {
                            result.pop = Some(decoded_val);
                        }
                    }
                    _ => {
                        let decoded_val = decode(value)
                            .map(|s| s)
                            .map_err(|_| Bip321Errors::InvalidEncoding)?;
                        extra_params
                            .entry(key)
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
                ext.handle_param(key, values)?
            }
        }

        if let Some(ext) = result.extras.as_ref() {
            if ext.is_empty() {
                result.extras = None;
            }
        }

        if result.address.is_none() && result.extras.is_none() {
            return Err(Bip321Errors::NoOnePaymentWasFound);
        }

        Ok(result)
    }

    pub fn build(&self) -> String {
        let mut uri = String::from("bitcoin:");

        if let Some(ref addr) = self.address {
            let address = addr.clone().assume_checked().to_string();
            uri.push_str(&format!("{}", address));
        }

        let mut params: Vec<String> = Vec::new();

        if let Some(amount) = self.amount {
            params.push(format!("amount={}", amount.to_btc()));
        }

        if let Some(label) = &self.label {
            params.push(format!("label={}", encode(label)));
        }

        if let Some(ref message) = self.message {
            params.push(format!("message={}", encode(message)));
        }

        if let Some(ref pop) = self.pop {
            params.push(format!("pop={}", encode(pop)));
        }

        if !params.is_empty() {
            uri.push('?');
            uri.push_str(&params.join("&"));
        }

        uri
    }
}
impl<'a, T: Bip321ExtraHandle<'a>> Bip321<'a, T, NetworkUnchecked> {
    pub fn into_checked(
        self,
        network: Network,
    ) -> Result<Bip321<'a, T, NetworkChecked>, Bip321Errors<'a>> {
        let checked_addr = match self.address {
            Some(addr) => {
                let checked = addr
                    .require_network(network)
                    .map_err(|_| Bip321Errors::InvalidAddress("Wrong Network"))?;
                Some(checked)
            }
            None => None,
        };

        if let Some(ext) = self.extras.as_ref() {
            ext.validate(network)?;
        }

        Ok(Bip321 {
            address: checked_addr,
            amount: self.amount,
            label: self.label,
            message: self.message,
            pop: self.pop,
            extras: self.extras,
        })
    }
}

impl<'a, T: Bip321ExtraHandle<'a>> Default for Bip321<'a, T, NetworkUnchecked> {
    fn default() -> Self {
        Bip321 {
            address: None,
            amount: None,
            label: None,
            message: None,
            pop: None,
            extras: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct ExtraExample {
    pub pj: Vec<String>,
    pub sp: Vec<String>,
    pub lightning: Vec<String>,
}

impl<'a> Bip321ExtraHandle<'a> for ExtraExample {
    fn handle_param(
        &mut self,
        key: &'a str,
        values: Vec<Cow<'a, str>>,
    ) -> Result<(), Bip321Errors<'a>> {
        match key {
            "pj" => {
                for val in values {
                    self.pj.push(val.to_string());
                }
                Ok(())
            }
            "lightning" => {
                for val in values {
                    self.lightning.push(val.to_string());
                }
                Ok(())
            }
            "sp" => {
                for val in values {
                    self.sp.push(val.to_string());
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn is_empty(&self) -> bool {
        self.pj.is_empty() && self.lightning.is_empty() && self.sp.is_empty()
    }

    fn is_supported_key(&self, key: &str) -> bool {
        matches!(key, "pj" | "lightning" | "sp")
    }
}
