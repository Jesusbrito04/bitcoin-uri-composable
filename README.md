# Bip321-Rust: Composable Bitcoin URI Parser
<p>A robust, type-safe, and highly extensible BIP-321 (Bitcoin URI) parser written in Rust. This library is designed to be the backbone of modern Bitcoin wallets that need to support not just simple on-chain payments, but also Payjoin, Silent Payments, Lightning Network, and beyond.<p>

## Features
- **Full BIP-321 Support:** Handles the standard ```bitcoin:``` scheme.
- **Composable Architecture:** Use the Bip321ExtraHandle trait to easily add support for custom query parameters (Payjoin, SP, LN, etc.) without modifying the core parser.
- **Strict Validation:**
  - Prevents duplicate parameters for standard keys (label, message, amount, pop).
  - Correctly handles req- (required) parameters for forward compatibility.
  - Validates Bitcoin amounts with 8-decimal precision.
  - Network Validation: Effortlessly verify if an address matches your wallet's current network (Mainnet, Testnet, etc.).

## 📦 Installation

```toml
[dependencies]
bitcoin = "0.32.8"
bitcoin-uri-composer = "0.1.0"
```

## 🛠 Usage
**Basic Parsing & Network Checking**
```rust
use bitcoin::Network;
use bitcoin_uri_composer::{Bip321, ExtraExample};

fn main() {
    let uri = "bitcoin:bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4?amount=1.5";
    
    // 1. Parse from string (returns Bip321<T, NetworkUnchecked>)
    let unchecked = Bip321::<ExtraExample>::parse_url(uri).expect("Invalid URI");
    
    // 2. Validate against your target network
    match unchecked.into_checked(Network::Bitcoin) {
        Ok(payment) => {
            println!("Address is valid for Mainnet: {:?}", payment.address);
            println!("Amount to send: {:?}", payment.amount);
        },
        Err(e) => eprintln!("Validation error: {:?}", e),
    }
}
```


**Advanced: Handling Extras (Lightning, Payjoin, Secret Payment)**
<p>The library allows you to define how to handle non-standard parameters.<p>

```rust
let uri = "bitcoin:?sp=sp1qsilentpayment&pj=https://payjoin.example.com";
let result = Bip321::<ExtraExample>::parse_url(uri).unwrap();

if let Some(extras) = result.extras {
    println!("Silent Payment: {:?}", extras.sp);
    println!("Payjoin Endpoints: {:?}", extras.pj);
}
```

## 🧩 The Extension System
**To support new payment protocols, simply implement the Bip321ExtraHandle trait for your own struct:**

```rust
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
```

## 🧩 How to Implement Your Own Handler
<p>The core strength of bitcoin-uri-composer is its extensibility. You can define exactly which extra parameters your wallet supports (like Payjoin, Lightning, or Silent Payments).<p>

**1. Define your data structure**
   <p>Create a struct that will hold your custom payment data:<p>

```rust
#[derive(Debug, Default)]
pub struct MyWalletExtras {
    pub payjoin_endpoints: Vec<String>,
    pub lightning_invoice: Vec<String>,
}
```
**2. Implement the Bip321ExtraHandle trait**
<p>This tells the parser how to fill your struct and which keys are "safe" to use.<p>

```rust
impl<'a> Bip321ExtraHandle<'a> for MyWalletExtras {
    // 1. Logic to store the parameters
    fn handle_param(&mut self, key: &'a str, values: Vec<Cow<'a, str>>) -> Result<(), Bip321Errors<'a>> {
        match key {
            "pj" => {
                for val in values {
                    self.payjoin_endpoints.push(val.to_string());
                };
                Ok(())
            }
            "lightning" => {
                for val in values {
                    self.lightning_invoice.push(val.to_string());
                };
                Ok(())
            }
            _ => Ok(()), // Ignore unknown non-required parameters
        }
    }

    fn validate(&self, _network: Network) -> Result<(), Bip321Errors> {
        if let Some(ref url) = self.payjoin_endpoints[0] {
            // Payjoin Rule: Must be HTTPS or .onion
            if !url.starts_with("https://") && !url.contains(".onion") {
                return Err(Bip321Errors::InvalidRequiredPayment);
            }
        }
        Ok(())
    }

    // 2. Security: Which keys do you support? 
    // If a 'req-key' is found and this returns false, parsing will fail.
    fn is_supported_key(&self, key: &str) -> bool {
        matches!(key, "pj" | "lightning")
    }

    // 3. Cleanup: If all fields are empty, the parser will return None for extras.
    fn is_empty(&self) -> bool {
        self.payjoin_endpoints.is_none() && self.lightning_invoice.is_none()
    }
}
```
**3. Use it with the parser**
<p>Now you can parse any Bitcoin URI using your custom logic:<p>

```rust
let uri = "bitcoin:address?pj=https://...&req-unknown=123";
// This will return Err(Bip321Errors::InvalidRequiredPayment) 
// because 'unknown' is required but not supported in our handler.
let result = Bip321::<MyWalletExtras>::parse_url(uri).unwrap();
```
