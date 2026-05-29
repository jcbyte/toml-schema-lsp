use std::path::Path;

use toml_schema::schema::Schema;

fn main() {
    let schema = Schema::load(Path::new("eg.tosd")).unwrap();
    let result = schema.validate_file(Path::new("a.toml"));

    println!("{:?}", result);
}
