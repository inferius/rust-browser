//! Generic Sensor API - Accelerometer, Gyroscope, Magnetometer, AmbientLightSensor,
//! LinearAccelerationSensor, GravitySensor, AbsoluteOrientationSensor.
//!
//! Spec: https://www.w3.org/TR/generic-sensor/

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SensorType {
    Accelerometer,
    LinearAcceleration,
    Gravity,
    Gyroscope,
    Magnetometer,
    AmbientLight,
    AbsoluteOrientation,
    RelativeOrientation,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SensorState {
    Idle,
    Activating,
    Activated,
    Errored,
}

#[derive(Debug, Clone)]
pub struct SensorReading {
    pub timestamp_ms: f64,
    /// Values - per sensor type:
    /// - Accelerometer/Gyro: (x, y, z)
    /// - AmbientLight: (lux, 0, 0)
    /// - Orientation: (alpha, beta, gamma) nebo quaternion
    pub values: Vec<f64>,
}

pub struct Sensor {
    pub sensor_type: SensorType,
    pub state: SensorState,
    pub frequency_hz: f32,       // requested frequency
    pub last_reading: Option<SensorReading>,
}

impl Sensor {
    pub fn new(sensor_type: SensorType, frequency_hz: f32) -> Self {
        Self {
            sensor_type,
            state: SensorState::Idle,
            frequency_hz,
            last_reading: None,
        }
    }

    pub fn start(&mut self) {
        self.state = SensorState::Activating;
    }

    pub fn activate(&mut self) {
        self.state = SensorState::Activated;
    }

    pub fn stop(&mut self) {
        self.state = SensorState::Idle;
    }

    pub fn push_reading(&mut self, values: Vec<f64>, timestamp_ms: f64) {
        self.last_reading = Some(SensorReading { timestamp_ms, values });
    }
}

#[derive(Default)]
pub struct SensorRegistry {
    pub sensors: HashMap<u64, Sensor>,
    pub next_id: u64,
}

impl SensorRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn create(&mut self, sensor_type: SensorType, frequency: f32) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.sensors.insert(id, Sensor::new(sensor_type, frequency));
        id
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Sensor> {
        self.sensors.get_mut(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_sensor() {
        let mut r = SensorRegistry::new();
        let id = r.create(SensorType::Accelerometer, 60.0);
        assert!(id > 0);
        let s = r.get_mut(id).unwrap();
        assert_eq!(s.sensor_type, SensorType::Accelerometer);
        assert_eq!(s.frequency_hz, 60.0);
    }

    #[test]
    fn sensor_lifecycle() {
        let mut s = Sensor::new(SensorType::Gyroscope, 30.0);
        assert_eq!(s.state, SensorState::Idle);
        s.start();
        assert_eq!(s.state, SensorState::Activating);
        s.activate();
        assert_eq!(s.state, SensorState::Activated);
        s.stop();
        assert_eq!(s.state, SensorState::Idle);
    }

    #[test]
    fn push_reading_stored() {
        let mut s = Sensor::new(SensorType::Magnetometer, 10.0);
        s.push_reading(vec![1.0, 2.0, 3.0], 100.0);
        let r = s.last_reading.as_ref().unwrap();
        assert_eq!(r.values, vec![1.0, 2.0, 3.0]);
        assert_eq!(r.timestamp_ms, 100.0);
    }
}
