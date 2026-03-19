# Bip321-Rust: Composable Bitcoin URI Parser
<p>A robust, type-safe, and highly extensible BIP-321 (Bitcoin URI) parser written in Rust. This library is designed to be the backbone of modern Bitcoin wallets that need to support not just simple on-chain payments, but also Payjoin, Silent Payments, Lightning Network, and beyond.<p>

## Features
- **Full BIP-321 Support:** Handles the standard ```bitcoin:``` scheme.
- **Composable Architecture:** Use the Bip321ExtraHandle trait to easily add support for custom query parameters (Payjoin, SP, LN, etc.) without modifying the core parser.
- **Strict Validation:**
  - Prevents duplicate parameters for standard keys (label, message, amount, pop).
  - Correctly handles req- (required) parameters for forward compatibility.
  - Validates Bitcoin amounts with 8-decimal precision.

## 📦 Installation

```toml
[dependencies]
bitcoin = "0.32.8"
bitcoin-uri-composer = "0.1.0"
```

## 🛠 Usage
**Basic Parsing**
```rust
use bitcoin_uri_composer::{ Bip321, ExtrasExample };

fn main() {
    let uri = "bitcoin:bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4?amount=1.5&label=Coffee";
    let payment = uri.parse::<Bip321<ExtraExample>>().unwrap();

    println!("Sending to: {:?}", payment.address);
    println!("Amount: {:?}", payment.amount);
    println!("Label: {:?}", payment.label);
}
```


**Advanced: Handling Extras (Lightning, Payjoin, Secret Payment)**
<p>The library allows you to define how to handle non-standard parameters.<p>

```rust
let uri = "bitcoin:?pj=https://endpoint1.com&lightning=lnbc1_invoice_test_vector&sp=sp1qsilentpayment";
let payment = uri.parse::<Bip321<ExtrasExample>>().unwrap();

if let Some(extras) = payment.extras {
    // Access your custom fields
    println!("Payjoin endpoints: {:?}", extras.pj);
    println!("Lightning Invoice: {:?}", extras.lightning);
    println!("Secret Payments: {:?}", extras.sp);
}
```

## 🧩 The Extension System
**To support new payment protocols, simply implement the Bip321ExtraHandle trait for your own struct:**

```rust
pub trait Bip321ExtraHandle: Default {
    fn handle_param(&mut self, key: &str, value: Vec<String>) -> Result<(), Bip321Errors>;
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
impl Bip321ExtraHandle for MyWalletExtras {
    // 1. Logic to store the parameters
    fn handle_param(&mut self, key: &str, values: Vec<String>) -> Result<(), Bip321Errors> {
        match key {
            "pj" => {
                self.payjoin_endpoints.extend(values);
                Ok(())
            }
            "lightning" => {
                self.lightning_invoice.extend(values);
                Ok(())
            }
            _ => Ok(()), // Ignore unknown non-required parameters
        }
    }

    // 2. Security: Which keys do you support? 
    // If a 'req-key' is found and this returns false, parsing will fail.
    fn is_supported_key(&self, key: &str) -> bool {
        matches!(key, "pj" | "lightning")
    }

    // 3. Cleanup: If all fields are empty, the parser will return None for extras.
    fn is_empty(&self) -> bool {
        self.payjoin_endpoints.is_empty() && self.lightning_invoice.is_none()
    }
}
```
**3. Use it with the parser**
<p>Now you can parse any Bitcoin URI using your custom logic:<p>

```rust
let uri = "bitcoin:address?pj=https://...&req-unknown=123";
// This will return Err(Bip321Errors::InvalidRequiredPayment) 
// because 'unknown' is required but not supported in our handler.
let result = uri.parse::<Bip321<MyWalletExtras>>();
```
