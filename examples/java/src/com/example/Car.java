package com.example;

public class Car {
    public static int wheelCount = 4;

    private String make;
    private String model;
    private int year;
    private CarType type;

    public enum CarType {
        SEDAN, SUV, TRUCK, COUPE
    }

    public Car(String make, String model, int year, CarType type) {
        this.make = make;
        this.model = model;
        this.year = year;
        this.type = type;
    }

    public String getMake() {
        return make;
    }

    public String getModel() {
        return model;
    }

    public int getYear() {
        return year;
    }

    public CarType getType() {
        return type;
    }

    public void displayInfo() {
        System.out.println("Car Information:");
        System.out.println("Make: " + make);
        System.out.println("Model: " + model);
        System.out.println("Year: " + year);
        System.out.println("Type: " + type);
    }
}
