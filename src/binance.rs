use std::time::{SystemTime, UNIX_EPOCH};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::Value;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct BinanceClient {
    api_key: String,
    api_secret: String,
    base_url: String,
    time_offset: i64,
    client: reqwest::Client,
}

impl BinanceClient {
    pub fn new(api_key: String, api_secret: String, testnet: bool) -> Self {
        let base_url = if testnet {
            "https://testnet.binancefuture.com".to_string()
        } else {
            "https://fapi.binance.com".to_string()
        };

        BinanceClient {
            api_key: api_key.trim().to_string(),
            api_secret: api_secret.trim().to_string(),
            base_url,
            time_offset: 0,
            client: reqwest::Client::new(),
        }
    }

    /// Sync local time with Binance server time to avoid timestamp issues (error -1021)
    pub async fn sync_time(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/fapi/v1/time", self.base_url);
        let resp = self.client.get(&url).send().await?.json::<Value>().await?;
        if let Some(server_time) = resp.get("serverTime").and_then(|v| v.as_i64()) {
            let local_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)?
                .as_millis() as i64;
            self.time_offset = server_time - local_time;
            println!("[BinanceClient] Time offset synced: {} ms", self.time_offset);
        }
        Ok(())
    }

    fn get_timestamp(&self) -> i64 {
        let local_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        local_time + self.time_offset
    }

    fn sign(&self, query: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(self.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(query.as_bytes());
        let result = mac.finalize();
        let code_bytes = result.into_bytes();
        hex::encode(code_bytes)
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-MBX-APIKEY",
            HeaderValue::from_str(&self.api_key).unwrap(),
        );
        headers
    }

    pub async fn place_limit_maker_order(
        &self,
        symbol: &str,
        side: &str,
        qty: f64,
        price: f64,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let path = "/fapi/v1/order";
        let timestamp = self.get_timestamp();
        
        // Post-Only / Limit Maker on Binance Futures is LIMIT order with timeInForce = GTX
        let query_without_sig = format!(
            "symbol={}&side={}&type=LIMIT&timeInForce=GTX&quantity={}&price={:.7}&timestamp={}&recvWindow=5000",
            symbol, side, qty, price, timestamp
        );
        let signature = self.sign(&query_without_sig);
        let url = format!("{}{}?{}&signature={}", self.base_url, path, query_without_sig, signature);

        let resp = self.client.post(&url)
            .headers(self.headers())
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;
        let json_body: Value = serde_json::from_str(&body)?;

        if !status.is_success() {
            return Err(format!("Binance API Error: status={} body={}", status, body).into());
        }

        Ok(json_body)
    }

    pub async fn cancel_order(
        &self,
        symbol: &str,
        order_id: i64,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let path = "/fapi/v1/order";
        let timestamp = self.get_timestamp();
        let query_without_sig = format!(
            "symbol={}&orderId={}&timestamp={}&recvWindow=5000",
            symbol, order_id, timestamp
        );
        let signature = self.sign(&query_without_sig);
        let url = format!("{}{}?{}&signature={}", self.base_url, path, query_without_sig, signature);

        let resp = self.client.delete(&url)
            .headers(self.headers())
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;
        let json_body: Value = serde_json::from_str(&body)?;

        if !status.is_success() {
            return Err(format!("Binance API Error: status={} body={}", status, body).into());
        }

        Ok(json_body)
    }

    pub async fn query_order(
        &self,
        symbol: &str,
        order_id: i64,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let path = "/fapi/v1/order";
        let timestamp = self.get_timestamp();
        let query_without_sig = format!(
            "symbol={}&orderId={}&timestamp={}&recvWindow=5000",
            symbol, order_id, timestamp
        );
        let signature = self.sign(&query_without_sig);
        let url = format!("{}{}?{}&signature={}", self.base_url, path, query_without_sig, signature);

        let resp = self.client.get(&url)
            .headers(self.headers())
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;
        let json_body: Value = serde_json::from_str(&body)?;

        if !status.is_success() {
            return Err(format!("Binance API Error: status={} body={}", status, body).into());
        }

        Ok(json_body)
    }
}
