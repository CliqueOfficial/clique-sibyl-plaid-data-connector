use std::prelude::v1::*;
use sibyl_base_data_connector::base::DataConnector;
use sibyl_base_data_connector::serde_json::json;
use std::string::ToString;
use sibyl_base_data_connector::serde_json::Value;
use std::str;
use String;
use std::panic;
use std::time::*;
// use std::untrusted::time::SystemTimeEx;
use sibyl_base_data_connector::utils::{parse_result_chunked, parse_result, tls_post};
use sibyl_base_data_connector::utils::simple_tls_client_no_cert_check;

// Plaid API

const SIGN_CLAIM_SGX_HOST: &'static str = "clique-signclaim";
const BALANCE_SUFFIX: &'static str = "/accounts/balance/get";
const LINK_TOKEN_SUFFIX: &'static str = "/link/token/create";
const EXCHANGE_ACCESS_TOKEN_SUFFIX: &'static str = "/item/public_token/exchange";
const SANDBOX_PUBLIC_TOKEN_SUFFIX: &'static str = "/sandbox/public_token/create";
const SANDBOX_EXCHANGE_ACCESS_TOKEN_SUFFIX: &'static str = "/item/public_token/exchange";
const SANDBOX_PLAID_HOST: &'static str = "sandbox.plaid.com";

pub struct PlaidConnector {

}

impl DataConnector for PlaidConnector {
    fn query(&self, query_type: &Value, query_param: &Value) -> Result<Value, String> {
        let query_type_str = match query_type.as_str() {
            Some(r) => r,
            _ => {
                let err = format!("query_type to str failed");
                println!("{:?}", err);
                return Err(err);
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

                let plaintext = match tls_post(SANDBOX_PLAID_HOST, &req, 443) {
                    Ok(r) => r,
                    Err(e) => {
                        let err = format!("tls_post to str: {:?}", e);
                        println!("{:?}", err);
                        return Err(err);
                    }
                };
                let mut reason = "".to_string();
                let mut result: Value = json!("fail");
                match parse_result(&plaintext) {
                    Ok(r) => {
                        result = r;
                    },
                    Err(e) => {
                        reason = e;
                    }
                }
                // println!("parse result {:?}", result);
                Ok(json!({
                    "result": result,
                    "reason": reason
                }))
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

                let plaintext = match tls_post(SANDBOX_PLAID_HOST, &req, 443) {
                    Ok(r) => r,
                    Err(e) => {
                        let err = format!("tls_post to str failed: {:?}", e);
                        println!("{:?}", err);
                        return Err(err);
                    }
                };
                let mut reason = "".to_string();
                let mut result: Value = json!("fail");
                match parse_result(&plaintext) {
                    Ok(r) => {
                        result = r;
                    },
                    Err(e) => {
                        reason = e;
                    }
                }
                // println!("parse result {:?}", result);
                Ok(json!({
                    "result": result,
                    "reason": reason
                }))
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
                        return Err(err);
                    }
                };
                let mut reason = "".to_string();
                let mut result: Value = json!("fail");
                match parse_result_chunked(&plaintext) {
                    Ok(resp_json) => {
                        result = match panic::catch_unwind(|| {
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
                                    upper
                                    SIGN_CLAIM_SGX_HOST
                                );
                                let zk_range_proof = simple_tls_client_no_cert_check(SIGN_CLAIM_SGX_HOST, &req, 12341).unwrap_or(json!({"result": {}}));
                                let zk = &zk_range_proof["result"];
                                let empty_arr: Vec<u8> = vec![];
                                return json!({
                                    "result": in_range,
                                    "zk_range_proof": {
                                        "proof": zk["proof"].as_array().unwrap_or(&empty_arr),
                                        "attestation": zk["attestation"].as_str().unwrap_or("")
                                    }
                                });
                            }
                            return json!("false");
                        }) {
                            Ok(r) => r,
                            Err(e) => {
                                let err = format!("balance range failed: {:?}", e);
                                println!("{:?}", err);
                                return Err(err);
                            }
                        };
                        
                    },
                    Err(e) => {
                        reason = e;
                    }
                }
                // println!("parse result {:?}", result);
                Ok(json!({
                    "result": result,
                    "reason": reason
                }))
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
                        return Err(err);
                    }
                };
                let mut reason = "".to_string();
                let mut result: Value = json!("fail");
                match parse_result_chunked(&plaintext) {
                    Ok(resp_json) => {
                        result = match panic::catch_unwind(|| {
                            for account in resp_json["accounts"].as_array().unwrap() {
                                let balance = account["balances"]["current"].as_f64().unwrap();
                                let upper = query_param["rangeUpperBound"].as_f64().unwrap();
                                let lower = query_param["rangeBottomBound"].as_f64().unwrap();
                                if balance <= upper && balance >= lower {
                                    return json!("true");
                                }
                            }
                            return json!("false");
                        }) {
                            Ok(r) => r,
                            Err(e) => {
                                let err = format!("balance range failed: {:?}", e);
                                println!("{:?}", err);
                                return Err(err);
                            }
                        };
                        
                    },
                    Err(e) => {
                        reason = e;
                    }
                }
                // println!("parse result {:?}", result);
                Ok(json!({
                    "result": result,
                    "reason": reason
                }))
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

                let mut start_time = SystemTime::now();
                let plaintext = match tls_post(SANDBOX_PLAID_HOST, &req, 443) {
                    Ok(r) => r,
                    Err(e) => {
                        let err = format!("tls_post failed: {:?}", e);
                        println!("{:?}", err);
                        return Err(err);
                    }
                };
                match start_time.elapsed() {
                    Ok(r) => {
                        println!("tls_post took {}s", r.as_micros() as f32 / 1000000.0);
                    },
                    Err(_) => ()
                }
                start_time = SystemTime::now();
                let mut reason = "".to_string();
                let mut result: Value = json!("fail");
                match parse_result(&plaintext) {
                    Ok(r) => {
                        result = r;
                    },
                    Err(e) => {
                        reason = e;
                    }
                }
                match start_time.elapsed() {
                    Ok(r) => {
                        println!("parse result took {}ms", r.as_micros());
                    },
                    Err(_) => ()
                }
                println!("parse result {:?}", result);
                Ok(json!({
                    "result": result,
                    "reason": reason
                })) 
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

                let plaintext = match tls_post(SANDBOX_PLAID_HOST, &req, 443) {
                    Ok(r) => r,
                    Err(e) => {
                        let err = format!("tls_post failed: {:?}", e);
                        println!("{:?}", err);
                        return Err(err);
                    }
                };
                let mut reason = "".to_string();
                let mut result: Value = json!("fail");
                match parse_result(&plaintext) {
                    Ok(r) => {
                        result = r;
                    },
                    Err(e) => {
                        reason = e;
                    }
                }
                // println!("parse result {:?}", result);
                Ok(json!({
                    "result": result,
                    "reason": reason
                })) 
   
            },
            _ => {
                Err(format!("Unexpected query_type: {:?}", query_type))
            }
        }
    }
}

