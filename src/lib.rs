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

        if let Some(ext) = result.extras.as_ref() {
            if ext.is_empty() {
                result.extras = None;
            }
        }

        Ok(result)
    }
}

#[derive(Debug, Default)]
pub struct MyExtras {
    pj: Vec<String>,
    sp: Vec<String>,
    lightning: Vec<String>,
}

impl Bip321ExtraHandle for MyExtras {
    fn handle_param(&mut self, key: &str, value: Vec<String>) -> Result<(), Bip321Errors> {
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
        self.pj.is_empty() | self.lightning.is_empty() | self.sp.is_empty()
    }

    fn is_supported_key(&self, key: &str) -> bool {
        matches!(key, "pj" | "lightning" | "sp")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    // Mainnet Network 
    const LEGACY_ADDR: &str = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";
    const P2SH_ADDR: &str = "3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy";
    const SEGWIT_ADDR: &str = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
    const TAPROOT_ADDR: &str = "bc1p0hc953htakzhgfvpju4q6d6y5kncvm87pjnzdj77wye23kqsln5sy4j69g";

    #[test]
    fn only_the_address() {
        let url_legacy = format!("bitcoin:{}", LEGACY_ADDR);
        let result_legacy = url_legacy.parse::<Bip321<MyExtras>>().unwrap();
        assert!(result_legacy.address.unwrap() == Address::from_str(LEGACY_ADDR).unwrap());
        assert!(result_legacy.extras.is_none());

        let url_p2sh = format!("bitcoin:{}", P2SH_ADDR);
        let result_p2sh = url_p2sh.parse::<Bip321<MyExtras>>().unwrap();
        assert!(result_p2sh.address.unwrap() == Address::from_str(P2SH_ADDR).unwrap());
        assert!(result_p2sh.extras.is_none());

        let url_segwit = format!("bitcoin:{}", SEGWIT_ADDR);
        let result_segwit = url_segwit.parse::<Bip321<MyExtras>>().unwrap();
        assert!(result_segwit.address.unwrap() == Address::from_str(SEGWIT_ADDR).unwrap());
        assert!(result_segwit.extras.is_none());

        let url_taproot = format!("bitcoin:{}", TAPROOT_ADDR);
        let result_taproot = url_taproot.parse::<Bip321<MyExtras>>().unwrap();
        assert!(result_taproot.address.unwrap() == Address::from_str(TAPROOT_ADDR).unwrap());
        assert!(result_taproot.extras.is_none());
    }

    #[test]
    fn only_params() {
        let url = format!(
            "bitcoin:?amount=1.5&label=Donation&sp=sp1qsilentpayment&pj=https://endpoint1.com&pj=https://endpoint2.com&lightning=lnbc1_invoice_test_vector",
        );
        let result = url.parse::<Bip321<MyExtras>>().unwrap();
        
        // Verify common params
        assert!(result.address.is_none());
        assert_eq!(result.amount.unwrap(), Amount::from_btc(1.5).unwrap());
        assert_eq!(result.label.unwrap(), "Donation");

        // Verify extra params
        assert_eq!(result.extras.as_ref().unwrap().sp[0], "sp1qsilentpayment");
        assert_eq!(result.extras.as_ref().unwrap().lightning[0], "lnbc1_invoice_test_vector");

        // Verify extra duplicate params
        assert_eq!(result.extras.as_ref().unwrap().pj[0], "https://endpoint1.com");
        assert_eq!(result.extras.as_ref().unwrap().pj[1], "https://endpoint2.com");
    }   
}
