//! # BIP-321 Bitcoin URI Library
//! 
//! This crate provides a flexible parser and builder for Bitcoin URIs according to [BIP-321](https://github.com/bitcoin/bips/blob/master/bip-0321.mediawiki).
//! It supports standard parameters like `address`, `amount`, and `message`, as well as "Proof of Payment" (POP) and custom extra parameters.

use bitcoin::{
    Address, Amount, Denomination, Network,
    address::{NetworkChecked, NetworkUnchecked, NetworkValidation},
};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use urlencoding::{decode, encode};

/// Errors encountered while parsing or validating a BIP-321 URI.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Bip321Errors<'a> {
    /// The same parameter was provided multiple times.
    DuplicateParam(&'a str),
    /// The URI does not start with the `bitcoin:` schema or contains forbidden protocols in POP.
    IncorrectSchema,
    /// The provided address is syntactically invalid or for the wrong network.
    InvalidAddress(&'a str),
    /// The amount is not a valid decimal or contains invalid characters (like commas).
    InvalidAmount,
    /// The URI contains neither a valid address nor valid extra payment data.
    NoOnePaymentWasFound,
    /// Failed to decode percent-encoded characters.
    InvalidEncoding,
    /// A parameter prefixed with `req-` was encountered but is not supported by the handler.
    InvalidRequiredPayment,
}

/// A trait to handle custom/extra parameters in a Bitcoin URI.
/// 
/// Implement this to support extensions like Payjoin (`pj`), Lightning (`lightning`), or Silent Payments (`sp`).
pub trait Bip321ExtraHandle<'a>
where
    Self: Default,
{
    /// Processes a key-value pair found in the URI query string.
    fn handle_param(
        &mut self,
        key: &'a str,
        value: Vec<Cow<'a, str>>,
    ) -> Result<(), Bip321Errors<'a>>;

    /// Optional validation step (e.g., checking address types for a specific network).
    fn validate(&self, _network: Network) -> Result<(), Bip321Errors<'a>> {
        Ok(())
    }

    /// Returns true if no extra data has been collected.
    fn is_empty(&self) -> bool;

    /// Checks if a specific key is supported. Used to validate `req-` (required) parameters.
    fn is_supported_key(&self, key: &str) -> bool;
}

/// Configuration for Proof of Payment (POP) parameters.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PopConfig<'a> {
    /// The base POP URI.
    pop: Cow<'a, str>,
    /// Whether this POP is mandatory (using the `req-pop` key).
    pub required: bool,
}

impl<'a> PopConfig<'a> {
    /// Appends payment data to the POP URI.
    /// 
    /// # Errors
    /// Returns `InvalidEncoding` if the `payment_data_hex` is not valid hex.
    pub fn finalize_uri(
        &self,
        source_key: &str,
        payment_data_hex: &str,
    ) -> Result<String, Bip321Errors<'a>> {
        if !payment_data_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Bip321Errors::InvalidEncoding);
        };

        let append_to_pop = format!("{}{}={}", self.pop, source_key, payment_data_hex);

        Ok(append_to_pop)
    }
}

/// The core BIP-321 structure representing a Bitcoin Payment Request.
/// 
/// `T` is the extra parameters handler, and `NetVal` tracks if the address network has been checked.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Bip321<'a, T, NetVal = NetworkUnchecked>
where
    T: Bip321ExtraHandle<'a>,
    NetVal: NetworkValidation,
{
    /// The destination Bitcoin address.
    pub address: Option<Address<NetVal>>,
    /// The amount to be paid.
    pub amount: Option<Amount>,
    /// A label for the receiver (e.g., "Satoshi Nakamoto").
    pub label: Option<Cow<'a, str>>,
    /// A message describing the transaction.
    pub message: Option<Cow<'a, str>>,
    /// Proof of Payment configuration.
    pub pop: Option<PopConfig<'a>>,
    /// Custom extra parameters.
    pub extras: Option<T>,
}

impl<'a, T: Bip321ExtraHandle<'a>> Bip321<'a, T> {
    /// Parses a string into a `Bip321` structure.
    /// 
    /// # Example
    /// ```
    /// # use bitcoin_uri_composer::{ Bip321, ExtraExample };
    /// let uri = "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?amount=20.3&label=Luke-Jr";
    /// let request: Bip321<ExtraExample> = Bip321::parse_url(uri).unwrap();
    /// assert_eq!(request.label.unwrap(), "Luke-Jr");
    /// ```
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
                            result.pop = if key != "pop" {
                                Some(PopConfig {
                                    pop: decoded_val,
                                    required: true,
                                })
                            } else {
                                Some(PopConfig {
                                    pop: decoded_val,
                                    required: false,
                                })
                            };
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

    /// Serializes the structure back into a standard `bitcoin:...` URI string.
    /// 
    /// # Example
    /// ```
    /// # use bitcoin_uri_composer::{Bip321, ExtraExample};
    /// # use std::borrow::Cow;
    /// let mut request: Bip321<ExtraExample> = Bip321::default();
    /// request.label = Some(Cow::Borrowed("Donation"));
    /// let uri = request.build();
    /// assert!(uri.contains("label=Donation"));
    /// ```
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

        if let Some(ref pop_conf) = self.pop {
            if pop_conf.required {
                params.push(format!("req-pop={}", encode(&pop_conf.pop)));
            } else {
                params.push(format!("pop={}", encode(&pop_conf.pop)));
            }
        }

        if !params.is_empty() {
            uri.push('?');
            uri.push_str(&params.join("&"));
        }

        uri
    }
}

impl<'a, T: Bip321ExtraHandle<'a>> Bip321<'a, T, NetworkUnchecked> {
    /// Validates the address against a specific Bitcoin network.
    /// 
    /// # Errors
    /// Returns `InvalidAddress` if the network does not match.
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

/// An example implementation of `Bip321ExtraHandle` supporting Payjoin, Lightning, and Silent Payments.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct ExtraExample {
    /// Payjoin URLs.
    pub pj: Vec<String>,
    /// Silent Payment addresses.
    pub sp: Vec<String>,
    /// Lightning Network invoices.
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