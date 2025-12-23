// Test file for ghost text autocomplete
// Try typing the first 2-3 characters of these words and see ghost suggestions

fn authentication_handler() {
    println!("authentication started");
}

fn authorization_check() {
    println!("authorization verified");
}

fn calculate_total(items: Vec<i32>) -> i32 {
    items.iter().sum()
}

fn calculate_average(items: Vec<i32>) -> f64 {
    let total = calculate_total(items.clone());
    total as f64 / items.len() as f64
}

fn process_request(request: String) {
    println!("processing: {}", request);
}

fn process_response(response: String) {
    println!("response: {}", response);
}

struct Configuration {
    database_url: String,
    database_port: u16,
    server_host: String,
    server_port: u16,
}

impl Configuration {
    fn new() -> Self {
        Configuration {
            database_url: String::from("localhost"),
            database_port: 5432,
            server_host: String::from("0.0.0.0"),
            server_port: 8080,
        }
    }
}

// Test suggestions:
// Type "auth" -> should suggest "entication" or "orization"
// Type "calc" -> should suggest "ulate_total" or "ulate_average"
// Type "proc" -> should suggest "ess_request" or "ess_response"
// Type "data" -> should suggest "base_url" or "base_port"
// Type "serv" -> should suggest "er_host" or "er_port"
// Type "Conf" -> should suggest "iguration"

fn main() {
    let config = Configuration::new();

    // Try typing here:
    //

}
