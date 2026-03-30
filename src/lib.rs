use bitcoin::{
    Address, Amount, Denomination, Network,
    address::{NetworkChecked, NetworkUnchecked, NetworkValidation},
};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use urlencoding::decode;

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
pub enum Bip321Errors<'a> {
    DuplicateParam(&'a str),
    IncorrectSchema,
    InvalidAddress(&'a str),
    InvalidAmount,
    NoOnePaymentWasFound,
    InvalidEncoding,
    InvalidRequiredPayment,
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

impl<'a, T: Bip321ExtraHandle<'a>> Bip321<'a, T, NetworkUnchecked> {
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
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct ExtraExample<'a> {
    pj: Vec<Cow<'a, str>>,
    sp: Vec<Cow<'a, str>>,
    lightning: Vec<Cow<'a, str>>,
}

impl<'a> Bip321ExtraHandle<'a> for ExtraExample<'a> {
    fn handle_param(
        &mut self,
        key: &'a str,
        value: Vec<Cow<'a, str>>,
    ) -> Result<(), Bip321Errors<'a>> {
        match key {
            "pj" => {
                self.pj.extend(value);
                Ok(())
            }
            "lightning" => {
                self.lightning.extend(value);
                Ok(())
            }
            "sp" => {
                self.sp.extend(value);
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

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::*;
    // Mainnet Network
    const LEGACY_ADDR: &str = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";
    const P2SH_ADDR: &str = "3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy";
    const SEGWIT_ADDR: &str = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
    const TAPROOT_ADDR: &str = "bc1p0hc953htakzhgfvpju4q6d6y5kncvm87pjnzdj77wye23kqsln5sy4j69g";

    #[test]
    fn only_the_address() {
        let testcase = vec![
            (format!("bitcoin:{}", LEGACY_ADDR), LEGACY_ADDR),
            (format!("bitcoin:{}", P2SH_ADDR), P2SH_ADDR),
            (format!("bitcoin:{}", SEGWIT_ADDR), SEGWIT_ADDR),
            (format!("bitcoin:{}", TAPROOT_ADDR), TAPROOT_ADDR),
        ];
        for (url, expected_address) in testcase {
            let result: Bip321<ExtraExample> = Bip321::parse_url(&url).unwrap();
            assert!(
                result.address.clone().unwrap() == Address::from_str(expected_address).unwrap()
            );

            let checked: Bip321<ExtraExample, NetworkChecked> =
                result.into_checked(Network::Bitcoin).unwrap();
            assert!(
                checked.address.unwrap()
                    == Address::from_str(expected_address)
                        .unwrap()
                        .assume_checked()
            );
            assert!(checked.extras.is_none());
        }
    }

    #[test]
    fn only_params() {
        let url = "bitcoin:?amount=1.5&label=Donation&sp=sp1qsilentpayment&pj=https://endpoint1.com&pj=https://endpoint2.com&lightning=lnbc1_invoice_test_vector";
        let result: Bip321<ExtraExample> = Bip321::parse_url(url).unwrap();

        // Verify common params
        assert!(result.address.is_none());
        assert_eq!(result.amount.unwrap(), Amount::from_btc(1.5).unwrap());
        assert_eq!(result.label.unwrap(), "Donation");

        // Verify extra params
        assert_eq!(result.extras.as_ref().unwrap().sp[0], "sp1qsilentpayment");
        assert_eq!(
            result.extras.as_ref().unwrap().lightning[0],
            "lnbc1_invoice_test_vector"
        );

        // Verify multiple query parameters with the same key MAY be included for query parameters representing payment instructions.
        assert_eq!(
            result.extras.as_ref().unwrap().pj[0],
            "https://endpoint1.com"
        );
        assert_eq!(
            result.extras.as_ref().unwrap().pj[1],
            "https://endpoint2.com"
        );
    }

    #[test]
    fn fail_on_duplicate_label_message_pop() {
        // Verify multiple query parameters with the same key MUST NOT be included for keys "label", "message", or "pop"
        let testcase = vec![
            ("bitcoin:?amount=1.5&label=Donation&label=Donation", "label"),
            (
                "bitcoin:?amount=1.5&message=Donation&message=Donation",
                "message",
            ),
            ("bitcoin:?amount=1.5&pop=Donation&pop=Donation", "pop"),
        ];

        for (url, expected_key) in testcase {
            let result: Result<Bip321<ExtraExample>, Bip321Errors> = Bip321::parse_url(url);

            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err(),
                Bip321Errors::DuplicateParam(expected_key)
            );
        }
    }

    #[test]
    fn error_on_missing_address_and_extras() {
        let url = "bitcoin:?amount=2.5&label=test";
        let result: Result<Bip321<ExtraExample>, Bip321Errors> = Bip321::parse_url(url);

        assert_eq!(result.unwrap_err(), Bip321Errors::NoOnePaymentWasFound);
    }

    #[test]
    fn error_on_missing_address_and_extras_unknown() {
        let url_unknown = "bitcoin:?unknown=123&another=456";
        let result: Result<Bip321<ExtraExample>, Bip321Errors> = Bip321::parse_url(url_unknown);

        assert_eq!(result.unwrap_err(), Bip321Errors::NoOnePaymentWasFound);
    }

    #[test]
    fn error_on_wrong_network() {
        // Mainnet Address
        let url = format!("bitcoin:{}", LEGACY_ADDR);
        let result: Bip321<ExtraExample> = Bip321::parse_url(&url).unwrap();

        // Trying to validate using Testnet
        let checked = result.into_checked(Network::Testnet);

        assert!(checked.is_err());
        assert_eq!(
            checked.unwrap_err(),
            Bip321Errors::InvalidAddress("Wrong Network")
        );
    }

    #[test]
    fn into_checked_works_without_address() {
        let url = "bitcoin:?lightning=lnbc1...";
        let result: Bip321<ExtraExample> = Bip321::parse_url(url).unwrap();

        // The address is empty. Therefore, nothing will be validated.
        let checked = result.into_checked(Network::Bitcoin);

        assert!(checked.is_ok());
        assert!(checked.unwrap().address.is_none());
    }

    #[test]
    fn testnet_address_validation() {
        let testnet_addr = "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx";
        let url = format!("bitcoin:{}", testnet_addr);

        let result: Bip321<ExtraExample> = Bip321::parse_url(&url).unwrap();

        // This will fail in Mainnet, but succeed in Testnet
        assert!(result.clone().into_checked(Network::Bitcoin).is_err());
        assert!(result.into_checked(Network::Testnet).is_ok());
    }

    #[test]
    fn error_on_forbidden_req_pop_schemes() {
        // BIP 321: A wallet MUST validate that the scheme is not http, https, file, javascript, or mailto.
        // Since 'req-pop' (required), the full URI should be considered invalid.

        let forbidden_testcases = vec![
            "bitcoin:1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa?req-pop=http://rastreador.com/ip",
            "bitcoin:1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa?req-pop=https://phishing.com/login",
            "bitcoin:1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa?req-pop=javascript:alert('hack')",
            "bitcoin:1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa?req-pop=file:///etc/shadow",
            "bitcoin:1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa?req-pop=mailto:scam@ataque.com",
        ];

        for url in forbidden_testcases {
            let result: Result<Bip321<ExtraExample>, Bip321Errors> = Bip321::parse_url(url);

            // This should fail.
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Bip321Errors::IncorrectSchema);
        }
    }
}
