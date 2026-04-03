use bitcoin::{Address, Amount, Network, address::NetworkChecked};
use bitcoin_uri_composer::{Bip321, Bip321Errors, ExtraExample};
use std::borrow::Cow;
use std::str::FromStr;

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

    #[test]
    fn url_encoding_in_label_and_message() {
        // BIP 321: The values ​​have to be encoded to (URL-encoded) correctly.
        let url = format!(
            "bitcoin:{}?label=Satoshi%20Nakamoto&message=Payment%20for%20services%21%3F",
            LEGACY_ADDR
        );
        let result: Bip321<ExtraExample> = Bip321::parse_url(&url).unwrap();

        assert_eq!(result.label.unwrap(), "Satoshi Nakamoto");
        assert_eq!(result.message.unwrap(), "Payment for services!?");
    }

    #[test]
    fn fail_on_unknown_required_parameter() {
        // BIP 321: If a parameter starts with "req-" and the software doesn't understand it,
        // MUST consider the entire URI as invalid.
        let url = format!("bitcoin:{}?req-future=12345", LEGACY_ADDR);
        let result: Result<Bip321<ExtraExample>, Bip321Errors> = Bip321::parse_url(&url);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Bip321Errors::InvalidRequiredPayment);
    }

    #[test]
    fn success_on_known_required_parameter() {
        let url = "bitcoin:?req-sp=sp1qsilentpayment";
        let result: Bip321<ExtraExample> = Bip321::parse_url(url).unwrap();

        assert!(result.address.is_none());
        assert_eq!(result.extras.as_ref().unwrap().sp[0], "sp1qsilentpayment");
    }

    #[test]
    fn invalid_amount_formats() {
        let bad_amounts = vec![
            format!("bitcoin:{}?amount=1,5", LEGACY_ADDR),
            format!("bitcoin:{}?amount=-0.5", LEGACY_ADDR),
            format!("bitcoin:{}?amount=1.5.0", LEGACY_ADDR),
            format!("bitcoin:{}?amount=abc", LEGACY_ADDR),
        ];

        for url in bad_amounts {
            let result: Result<Bip321<ExtraExample>, Bip321Errors> = Bip321::parse_url(&url);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), Bip321Errors::InvalidAmount);
        }
    }

    #[test]
    fn fail_on_duplicate_amount() {
        let url = format!("bitcoin:{}?amount=1.0&amount=2.0", LEGACY_ADDR);
        let result: Result<Bip321<ExtraExample>, Bip321Errors> = Bip321::parse_url(&url);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Bip321Errors::DuplicateParam("amount"));
    }

    #[test]
    fn test_to_url_standard_serialization() {
        let addr = Address::from_str(SEGWIT_ADDR).unwrap();
        // 150,000,000 sats = 1.5 BTC
        let amount = Amount::from_sat(150_000_000);

        let bip321: Bip321<ExtraExample> = Bip321 {
            address: Some(addr),
            amount: Some(amount),
            label: Some(Cow::Borrowed("Satoshi Nakamoto")),
            message: Some(Cow::Borrowed("Payment & Donation!")),
            pop: None,
            extras: None,
        };

        let generated_url = bip321.build();

        // Check scheme and address
        assert!(generated_url.starts_with(&format!("bitcoin:{}", SEGWIT_ADDR)));
        // Check amount formatting (must be 1.5, not 150000000)
        assert!(generated_url.contains("amount=1.5"));
        // Check URL encoding (Space -> %20, & -> %26, ! -> %21)
        assert!(generated_url.contains("label=Satoshi%20Nakamoto"));
        assert!(generated_url.contains("message=Payment%20%26%20Donation%21"));
    }

    #[test]
    fn test_serialization_roundtrip() {
        // Test: Parse -> Serialize -> Parse
        let original_uri = "bitcoin:bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4?amount=2.1&label=Coffee%20Shop&message=Extra%20Sugar";

        // Parse initial string
        let parsed: Bip321<ExtraExample> = Bip321::parse_url(original_uri).expect("Should parse");

        // Serialize back to string
        let serialized = parsed.build();

        // Parse our own generated string
        let roundtripped: Bip321<ExtraExample> =
            Bip321::parse_url(&serialized).expect("Should parse generated URL");

        // Internal data must match exactly
        assert_eq!(parsed.address, roundtripped.address);
        assert_eq!(parsed.amount, roundtripped.amount);
        assert_eq!(parsed.label, roundtripped.label);
        assert_eq!(parsed.message, roundtripped.message);
    }

    #[test]
    fn test_to_url_no_address_only_params() {
        // BIP-321 allows URIs with only parameters (no specific address)
        let bip321: Bip321<ExtraExample> = Bip321 {
            address: None,
            amount: Some(Amount::from_btc(0.001).unwrap()),
            label: Some(Cow::Borrowed("Donation")),
            message: None,
            pop: None,
            extras: None,
        };

        let url = bip321.build();
        assert!(url.starts_with("bitcoin:?"));
        assert!(url.contains("amount=0.001"));
        assert!(url.contains("label=Donation"));
    }

    #[test]
    fn test_pop_uri_finalization_success() {
        let url = format!(
            "bitcoin:{}?req-pop=mywallet%3A%2F%2Fcallback%3Fid%3D123%26",
            LEGACY_ADDR
        );

        let result: Bip321<ExtraExample> = Bip321::parse_url(&url).unwrap();
        let pop_config = result.pop.expect("PopConfig should be present");

        let tx_hex = "01000000018a";
        let final_uri = pop_config
            .finalize_uri("onchain", tx_hex)
            .expect("Should finalize successfully");

        assert_eq!(final_uri, "mywallet://callback?id=123&onchain=01000000018a");
    }

    #[test]
    fn test_pop_invalid_hex_data() {
        let url = format!("bitcoin:{}?pop=app%3A%2F%2Fcallback%3F", LEGACY_ADDR);
        let result: Bip321<ExtraExample> = Bip321::parse_url(&url).unwrap();
        let pop_config = result.pop.unwrap();

        // Providing non-hex data should trigger an error
        let result = pop_config.finalize_uri("onchain", "not_a_hex_string");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Bip321Errors::InvalidEncoding);
    }

    #[test]
    fn test_required_vs_optional_pop() {
        let req_url = format!("bitcoin:{}?req-pop=app%3A%2F%2Fcb", LEGACY_ADDR);
        let opt_url = format!("bitcoin:{}?pop=app%3A%2F%2Fcb", LEGACY_ADDR);

        let res_req: Bip321<ExtraExample> = Bip321::parse_url(&req_url).unwrap();
        let res_opt: Bip321<ExtraExample> = Bip321::parse_url(&opt_url).unwrap();

        assert!(
            res_req.pop.unwrap().required,
            "req-pop must set required to true"
        );
        assert!(
            !res_opt.pop.unwrap().required,
            "pop must set required to false"
        );
    }

    #[test]
    fn test_bitcoin_scheme_is_case_insensitive() {
        let urls = vec![
            format!("BITCOIN:{}", LEGACY_ADDR),
            format!("BitCoin:{}", LEGACY_ADDR),
            format!("bitcoin:{}", LEGACY_ADDR),
        ];

        for url in urls {
            let result: Result<Bip321<ExtraExample>, Bip321Errors> = Bip321::parse_url(&url);
            assert!(
                result.is_ok(),
                "Scheme should be case-insensitive for: {}",
                url
            );
        }
    }

    #[test]
    fn test_empty_address_with_valid_extra_payment_instruction() {
        // BIP-321 allows empty address if at least one payment instruction (like lightning) exists
        let url = "bitcoin:?lightning=lnbc10u1pvjlutp...";
        let result = Bip321::<ExtraExample>::parse_url(url);

        assert!(result.is_ok());
        let res = result.unwrap();
        assert!(res.address.is_none());
        assert!(!res.extras.as_ref().unwrap().lightning.is_empty());
    }

    #[test]
    fn test_pop_forbidden_schemes_detection() {
        // Even if encoded, forbidden schemes must be rejected
        let url = format!("bitcoin:{}?pop=javascript%3Aalert(1)", LEGACY_ADDR);
        let result = Bip321::<ExtraExample>::parse_url(&url);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), Bip321Errors::IncorrectSchema);
    }
}
