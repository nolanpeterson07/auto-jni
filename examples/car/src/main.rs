include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

fn main() {
    let make = java().new_string("Toyota").unwrap();
    let model = java().new_string("Camry").unwrap();
    let car_type = com_example_Car::com_example_Car_CarType_from_str("SEDAN").unwrap();

    let car = com_example_Car::new(&make, &model, 2024, &car_type).unwrap();

    car.displayInfo().unwrap();
}
