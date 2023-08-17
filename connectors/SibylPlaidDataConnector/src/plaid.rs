use std::prelude::v1::*;
use sibyl_base_data_connector::base::DataConnector;
use sibyl_base_data_connector::errors::NetworkError;
use sibyl_base_data_connector::serde_json::json;
use std::string::ToString;
use sibyl_base_data_connector::serde_json::Value;
use std::str;
use std::panic;
use sibyl_base_data_connector::utils::{parse_result_chunked, tls_post, simple_tls_client, simple_tls_client_no_cert_check};
use once_cell::sync::Lazy;
use std::sync::Arc;
use rsa::{RSAPrivateKey, PaddingScheme};

static RSA_PRIVATE_KEY: Lazy<Arc<RSAPrivateKey>> = Lazy::new(|| {
    let mut rng = rand::rngs::OsRng::default();
    let bits = 2048;
    let key = RSAPrivateKey::new(&mut rng, bits).expect("failed to generate a key");

    Arc::new(key)
});

// Plaid API
const SIGN_CLAIM_SGX_HOST: &'static str = "clique-sign-claim";
const BALANCE_SUFFIX: &'static str = "/accounts/balance/get";
const LINK_TOKEN_SUFFIX: &'static str = "/link/token/create";
const EXCHANGE_ACCESS_TOKEN_SUFFIX: &'static str = "/item/public_token/exchange";
const SANDBOX_PUBLIC_TOKEN_SUFFIX: &'static str = "/sandbox/public_token/create";
const SANDBOX_EXCHANGE_ACCESS_TOKEN_SUFFIX: &'static str = "/item/public_token/exchange";
const SANDBOX_PLAID_HOST: &'static str = "sandbox.plaid.com";

pub struct PlaidConnector {

}

impl DataConnector for PlaidConnector {
    fn query(&self, query_type: &Value, query_param: &Value) -> Result<Value, NetworkError> {
        let query_type_str = match query_type.as_str() {
            Some(r) => r,
            _ => {
                let err = format!("query_type to str failed");
                println!("{:?}", err);
                return Err(NetworkError::String(err));
            }
        };
        match query_type_str {
            "plaid_link_token" => {
                let encoded_json = json!({
                    "client_id": query_param["clientId"],
                    "secret": query_param["secret"],
                    "user": {
                        "client_user_id": query_param["clientUserId"],
                    },
                    "client_name": "Clique2046",
                    "products": ["auth"],
                    "country_codes": ["US"],
                    "language": "en",
                    "redirect_uri": query_param["redirectUri"]
                }).to_string();
                let req = format!(
                    "POST {} HTTP/1.1\r\n\
                    HOST: {}\r\n\
                    User-Agent: curl/7.79.1\r\n\
                    Accept: */*\r\n\
                    Content-Type: application/json\r\n\
                    Content-Length: {}\r\n\r\n\
                    {}",
                    LINK_TOKEN_SUFFIX,
                    SANDBOX_PLAID_HOST,
                    encoded_json.len(),
                    encoded_json
                );
                simple_tls_client(SANDBOX_PLAID_HOST, &req, 443)
            },
            "plaid_exchange_access_token" => {
                let encoded_json = json!({
                    "client_id": query_param["clientId"],
                    "secret": query_param["secret"],
                    "public_token": query_param["publicToken"],
                }).to_string();
                let req = format!(
                    "POST {} HTTP/1.1\r\n\
                    HOST: {}\r\n\
                    User-Agent: curl/7.79.1\r\n\
                    Accept: */*\r\n\
                    Content-Type: application/json\r\n\
                    Content-Length: {}\r\n\r\n\
                    {}",
                    EXCHANGE_ACCESS_TOKEN_SUFFIX,
                    SANDBOX_PLAID_HOST,
                    encoded_json.len(),
                    encoded_json
                );
                simple_tls_client(SANDBOX_PLAID_HOST, &req, 443)
            },
            "plaid_bank_balance_range_zkp" => {
                let encoded_json = json!({
                    "client_id": query_param["clientId"],
                    "secret": query_param["secret"],
                    "access_token": query_param["accessToken"],
                }).to_string();
                let req = format!(
                    "POST {} HTTP/1.1\r\n\
                    HOST: {}\r\n\
                    User-Agent: curl/7.79.1\r\n\
                    Accept: */*\r\n\
                    Content-Type: application/json\r\n\
                    Content-Length: {}\r\n\r\n\
                    {}",
                    BALANCE_SUFFIX,
                    SANDBOX_PLAID_HOST,
                    encoded_json.len(),
                    encoded_json
                );
                let plaintext = match tls_post(SANDBOX_PLAID_HOST, &req, 443) {
                    Ok(r) => r,
                    Err(e) => {
                        let err = format!("tls_post to str: {:?}", e);
                        println!("{:?}", err);
                        return Err(NetworkError::String(err));
                    }
                };
                match parse_result_chunked(SANDBOX_PLAID_HOST, &plaintext) {
                    Ok(resp_json) => {
                        match panic::catch_unwind(|| {
                            for account in resp_json["accounts"].as_array().unwrap() {
                                let balance = account["balances"]["current"].as_u64().unwrap();
                                let upper = query_param["rangeUpperBound"].as_u64().unwrap();
                                let lower = query_param["rangeBottomBound"].as_u64().unwrap();
                                let in_range = balance <= upper && balance >= lower;
                                let req = format!(
                                    "GET /zkRangeProof?value={}&lower={}&upper={} HTTP/1.1\r\n\
                                    HOST: {}\r\n\
                                    User-Agent: curl/7.79.1\r\n\
                                    Accept: */*\r\n\r\n",
                                    balance,
                                    lower,
                                    upper,
                                    SIGN_CLAIM_SGX_HOST
                                );
                                let zk_range_proof = simple_tls_client_no_cert_check(
                                    SIGN_CLAIM_SGX_HOST, 
                                    &req, 
                                    12341
                                ).unwrap_or(json!({"result": {}}));
                                let zk = &zk_range_proof["result"];
                                let empty_arr: Vec<Value> = vec![];
                                return json!({
                                    "result": in_range,
                                    "zk_range_proof": {
                                        "proof": zk["proof"].as_array().unwrap_or(&empty_arr),
                                        "attestation": zk["attestation"].as_str().unwrap_or("")
                                    }
                                });
                            }
                            json!("false")
                        }) {
                            Ok(r) => Ok(r),
                            Err(e) => {
                                let err = format!("balance range failed: {:?}", e);
                                println!("{:?}", err);
                                Err(NetworkError::String(err))
                            }
                        }
                    },
                    Err(e) => {
                        Err(e)
                    }
                }
            },
            "plaid_bank_balance_range" => {
                let encoded_json = json!({
                    "client_id": query_param["clientId"],
                    "secret": query_param["secret"],
                    "access_token": query_param["accessToken"],
                }).to_string();
                let req = format!(
                    "POST {} HTTP/1.1\r\n\
                    HOST: {}\r\n\
                    User-Agent: curl/7.79.1\r\n\
                    Accept: */*\r\n\
                    Content-Type: application/json\r\n\
                    Content-Length: {}\r\n\r\n\
                    {}",
                    BALANCE_SUFFIX,
                    SANDBOX_PLAID_HOST,
                    encoded_json.len(),
                    encoded_json
                );
                let plaintext = match tls_post(SANDBOX_PLAID_HOST, &req, 443) {
                    Ok(r) => r,
                    Err(e) => {
                        let err = format!("tls_post to str: {:?}", e);
                        println!("{:?}", err);
                        return Err(NetworkError::String(err));
                    }
                };
                match parse_result_chunked(SANDBOX_PLAID_HOST, &plaintext) {
                    Ok(resp_json) => {
                        match panic::catch_unwind(|| {
                            for account in resp_json["accounts"].as_array().unwrap() {
                                let balance = account["balances"]["current"].as_f64().unwrap();
                                let upper = query_param["rangeUpperBound"].as_f64().unwrap();
                                let lower = query_param["rangeBottomBound"].as_f64().unwrap();
                                if balance <= upper && balance >= lower {
                                    return json!("true");
                                }
                            }
                            json!("false")
                        }) {
                            Ok(r) => Ok(r),
                            Err(e) => {
                                let err = format!("balance range failed: {:?}", e);
                                println!("{:?}", err);
                                Err(NetworkError::String(err))
                            }
                        }
                    },
                    Err(e) => {
                        Err(e)
                    }
                }
            },
            "plaid_sandbox_public_token" => {
                let encoded_json = json!({
                    "client_id": query_param["clientId"],
                    "secret": query_param["secret"],
                    "institution_id": query_param["institutionId"],
                    "initial_products": ["auth"],
                    "options": {
                        "webhook": "https://eoti3zo8yt7vmo.m.pipedream.net",
                        "override_username": "user_good",
                        "override_password": "pass_good"
                    }
                }).to_string();
                let req = format!(
                    "POST {} HTTP/1.1\r\n\
                    HOST: {}\r\n\
                    User-Agent: curl/7.79.1\r\n\
                    Accept: */*\r\n\
                    Content-Type: application/json\r\n\
                    Content-Length: {}\r\n\r\n\
                    {}",
                    SANDBOX_PUBLIC_TOKEN_SUFFIX,
                    SANDBOX_PLAID_HOST,
                    encoded_json.len(),
                    encoded_json
                );
                simple_tls_client(SANDBOX_PLAID_HOST, &req, 443)
            },
            "plaid_sandbox_public_token_encrypted_secret" => {
                let encrypted_secret: Vec<u8> = query_param["encrypted_secret"].as_array().unwrap().iter().map(
                    |x| x.as_u64().unwrap() as u8
                ).collect();
                let rsa_key = Arc::clone(&*RSA_PRIVATE_KEY);
                let dec_data = rsa_key.decrypt(
                    PaddingScheme::PKCS1v15, &encrypted_secret).expect("failed to decrypt");
                let secret = std::str::from_utf8(&dec_data).unwrap();
                let encoded_json = json!({
                    "client_id": query_param["clientId"],
                    "secret": secret,
                    "institution_id": query_param["institutionId"],
                    "initial_products": ["auth"],
                    "options": {
                        "webhook": "https://eoti3zo8yt7vmo.m.pipedream.net",
                        "override_username": "user_good",
                        "override_password": "pass_good"
                    }
                }).to_string();
                let req = format!(
                    "POST {} HTTP/1.1\r\n\
                    HOST: {}\r\n\
                    User-Agent: curl/7.79.1\r\n\
                    Accept: */*\r\n\
                    Content-Type: application/json\r\n\
                    Content-Length: {}\r\n\r\n\
                    {}",
                    SANDBOX_PUBLIC_TOKEN_SUFFIX,
                    SANDBOX_PLAID_HOST,
                    encoded_json.len(),
                    encoded_json
                );
                simple_tls_client(SANDBOX_PLAID_HOST, &req, 443)
            },
            "plaid_sandbox_exchange_access_token" => {
                let encoded_json = json!({
                    "client_id": query_param["clientId"],
                    "secret": query_param["secret"],
                    "public_token": query_param["publicToken"],
                }).to_string();
                let req = format!(
                    "POST {} HTTP/1.1\r\n\
                    HOST: {}\r\n\
                    User-Agent: curl/7.79.1\r\n\
                    Accept: */*\r\n\
                    Content-Type: application/json\r\n\
                    Content-Length: {}\r\n\r\n\
                    {}",
                    SANDBOX_EXCHANGE_ACCESS_TOKEN_SUFFIX,
                    SANDBOX_PLAID_HOST,
                    encoded_json.len(),
                    encoded_json
                );
                simple_tls_client(SANDBOX_PLAID_HOST, &req, 443)
            },
            "plaid_get_rsa_public_key" => {
                let pub_key = Arc::clone(&*RSA_PRIVATE_KEY).to_public_key();
                Ok(json!(format!("{:?}", pub_key)))
            },
            _ => {
                Err(NetworkError::String(format!("Unexpected query_type: {:?}", query_type)))
            }
        }
    }
}
