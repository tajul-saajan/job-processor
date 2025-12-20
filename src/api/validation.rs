use actix_web::HttpResponse;
use serde::Serialize;

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub fields: serde_json::Value,
}

/// Creates a configured JsonConfig with standardized error handling for the entire project
pub fn json_config() -> actix_web_validator::JsonConfig {
    actix_web_validator::JsonConfig::default()
        .error_handler(|err, _req| {
            let mut fields = serde_json::Map::new();

            match err {
                actix_web_validator::Error::Validate(validation_errors) => {
                    for (field, errors) in validation_errors.field_errors() {
                        let messages: Vec<String> = errors
                            .iter()
                            .map(|e| {
                                e.message
                                    .as_ref()
                                    .map(|m| m.to_string())
                                    .unwrap_or_else(|| format!("Validation error in field: {}", field))
                            })
                            .collect();
                        fields.insert(
                            field.to_string(),
                            serde_json::json!({"errors": messages})
                        );
                    }

                    let error_response = ErrorResponse {
                        error: "Validation failed".to_string(),
                        fields: serde_json::Value::Object(fields),
                    };
                    actix_web::error::InternalError::from_response(
                        "",
                        HttpResponse::BadRequest().json(error_response)
                    ).into()
                }
                actix_web_validator::Error::Deserialize(de_err) => {
                    let err_string = de_err.to_string();

                    if err_string.contains("EOF while parsing") {
                        fields.insert(
                            "message".to_string(),
                            serde_json::json!("Request body is empty. Expected JSON payload")
                        );
                    } else if err_string.contains("unknown variant") {
                        // Extract field name if possible, otherwise use generic message
                        fields.insert(
                            "message".to_string(),
                            serde_json::json!("Invalid enum value. Check allowed values for this field")
                        );
                    } else {
                        fields.insert(
                            "message".to_string(),
                            serde_json::json!("Invalid JSON format")
                        );
                    }

                    let error_response = ErrorResponse {
                        error: "Request validation failed".to_string(),
                        fields: serde_json::Value::Object(fields),
                    };
                    actix_web::error::InternalError::from_response(
                        "",
                        HttpResponse::BadRequest().json(error_response)
                    ).into()
                }
                _ => {
                    fields.insert(
                        "message".to_string(),
                        serde_json::json!("Validation error")
                    );

                    let error_response = ErrorResponse {
                        error: "Validation failed".to_string(),
                        fields: serde_json::Value::Object(fields),
                    };
                    actix_web::error::InternalError::from_response(
                        "",
                        HttpResponse::BadRequest().json(error_response)
                    ).into()
                }
            }
        })
}
