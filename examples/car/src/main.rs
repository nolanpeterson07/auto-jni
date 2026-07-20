include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

fn main() {
    let make = java().new_string("Toyota").unwrap();
    let model = java().new_string("Camry").unwrap();
    let car_type = com_example_Car::com_example_Car_CarType_from_str("SEDAN").unwrap();

    let car = com_example_Car::new(&make, &model, 2024, &car_type).unwrap();

    car.displayInfo().unwrap();

    println!("Wheels: {}", com_example_Car::get_wheelCount().unwrap());

    car.set_year(2025).unwrap();
    com_example_Car::set_wheelCount(6).unwrap();

    car.displayInfo().unwrap();
    println!("Wheels: {}", com_example_Car::get_wheelCount().unwrap());

    match com_example_Car::com_example_Car_CarType_from_str("HOVERCRAFT") {
        Ok(_) => unreachable!("HOVERCRAFT isn't a real CarType"),
        Err(e) => println!("Got expected error: {e}"),
    }

    car.displayInfo().unwrap();
}
