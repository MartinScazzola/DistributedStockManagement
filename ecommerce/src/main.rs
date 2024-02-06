use ecommerce::{ecommerce::Ecommerce, ecommerce_error::EcommerceError};
use std::env;

fn args_parser() -> Result<(usize, String), EcommerceError> {
    let args: Vec<String> = env::args().collect();

    let shop_amount: usize = args[1]
        .parse()
        .map_err(|_| EcommerceError::ParseArgs("Fallo parseando el shop amount".to_string()))?;
    let orders_path: String = args[2].to_string();

    Ok((shop_amount, orders_path))
}

fn main() -> Result<(), EcommerceError> {
    let (shop_amount, orders_path) = args_parser()?;

    let mut ecommerce = Ecommerce::new();

    ecommerce.make_orders(orders_path, shop_amount)?;

    Ok(())
}
