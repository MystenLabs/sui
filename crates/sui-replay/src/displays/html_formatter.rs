// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub fn generate_html_from_json(json_data: &String) -> String {
    let data: serde_json::Value = serde_json::from_str(json_data).unwrap();
    format!(
        r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
          <meta charset="UTF-8">
          <meta name="viewport" content="width=device-width, initial-scale=1.0">
          <title>JSON Data</title>
        </head>
        <body>
          <h1>Transaction Information</h1>
          <ul>
            <li>Transaction Information: {}</li>
            <li>Gas Status: {}</li>
            <li>Effects: {}</li>
           
          </ul>
        </body>
        </html>
        "#,
        data["transaction_info"].to_string(),
        data["gas_status"].to_string(),
        data["effects"].to_string()
    )
}
