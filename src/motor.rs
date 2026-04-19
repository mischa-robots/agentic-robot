// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>

//! Motor control integration with the `robot-control` crate.
//!
//! Provides a thin wrapper that adds speed limiting on top of
//! the `robot-control` library's `Robot` struct.
//!
//! IMPORTANT: The motors have a minimum effective speed of ~0.5.
//! Values below 0.5 cannot move the robot (it's too heavy) and risk
//! overloading/burning the motors. The controller enforces this:
//! - Inputs between -0.5 and 0.5 (exclusive) are treated as STOP (0.0)
//! - Inputs outside that dead zone are clamped to max_speed

use std::sync::Arc;

use robot_control::{Motor, MotorDriver, Robot, RobotConfig};
use tokio::sync::Mutex;
use tracing::info;

use crate::error::AppError;

/// Minimum motor speed that can actually move the robot.
/// Below this, the motors stall and risk overheating/burning.
pub const MIN_SPEED: f32 = 0.5;

/// Motor controller with speed limiting and dead zone protection.
pub struct MotorController {
    robot: Arc<Robot>,
    max_speed: f32,
}

impl MotorController {
    /// Create a new motor controller.
    ///
    /// # Arguments
    /// * `driver` - The motor driver implementation
    /// * `left_factor` - Left motor polarity factor (-1.0 or 1.0 to reverse direction)
    /// * `right_factor` - Right motor polarity factor (-1.0 or 1.0 to reverse direction)
    /// * `max_speed` - Maximum allowed speed (0.5 to 1.0)
    pub fn new(
        driver: Arc<Mutex<dyn MotorDriver>>,
        left_factor: f32,
        right_factor: f32,
        max_speed: f32,
    ) -> Result<Self, AppError> {
        let config = RobotConfig {
            left_motor_factor: left_factor,
            right_motor_factor: right_factor,
            left_motor: Motor::Motor1,
            right_motor: Motor::Motor2,
            ..RobotConfig::default()
        };

        let robot = Robot::new(driver, config)?;
        let max_speed = max_speed.clamp(MIN_SPEED, 1.0);

        info!(max_speed, min_speed = MIN_SPEED, "motor controller initialized");

        Ok(Self { robot, max_speed })
    }

    /// Drive with dead zone and speed limiting applied.
    ///
    /// Speed values between -0.5 and 0.5 are treated as 0 (stop that motor).
    /// Values outside the dead zone are clamped between MIN_SPEED and max_speed.
    pub async fn drive(&self, left: f32, right: f32) -> Result<(), AppError> {
        let left = Self::apply_dead_zone(left, self.max_speed);
        let right = Self::apply_dead_zone(right, self.max_speed);
        self.robot.drive(left, right).await?;
        Ok(())
    }

    /// Apply dead zone: values below MIN_SPEED become 0, above get clamped to max.
    fn apply_dead_zone(value: f32, max_speed: f32) -> f32 {
        if value.abs() < MIN_SPEED {
            0.0
        } else if value > 0.0 {
            value.clamp(MIN_SPEED, max_speed)
        } else {
            value.clamp(-max_speed, -MIN_SPEED)
        }
    }

    /// Stop all motors immediately.
    pub async fn stop(&self) -> Result<(), AppError> {
        self.robot.stop().await?;
        Ok(())
    }

    /// Shutdown the motor controller and release hardware.
    pub async fn shutdown(&self) {
        self.robot.shutdown().await;
    }

    /// Get the configured max speed.
    pub fn max_speed(&self) -> f32 {
        self.max_speed
    }
}

/// Create a motor driver for real hardware (PCA9685).
#[cfg(feature = "pca9685")]
pub fn create_hardware_driver(
    i2c_bus: &str,
    i2c_addr: u8,
) -> Result<Arc<Mutex<dyn MotorDriver>>, AppError> {
    let board = robot_control::Pca9685MotorBoard::new(i2c_bus, i2c_addr)
        .map_err(|e| AppError::Motor(robot_control::RobotError::Motor(e)))?;
    Ok(Arc::new(Mutex::new(board)))
}

/// Stub for non-hardware builds.
#[cfg(not(feature = "pca9685"))]
pub fn create_hardware_driver(
    _i2c_bus: &str,
    _i2c_addr: u8,
) -> Result<Arc<Mutex<dyn MotorDriver>>, AppError> {
    Err(AppError::Other(
        "hardware motor driver requires pca9685 feature".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use robot_control::motor::{Motor, MotorDriver};
    use robot_control::error::MotorError;

    /// Simple mock motor driver for testing.
    struct TestMockDriver;

    impl MotorDriver for TestMockDriver {
        fn set_motor_speed(&self, _motor: Motor, _speed: u16, _forward: bool) -> Result<(), MotorError> {
            Ok(())
        }
        fn stop_motor(&self, _motor: Motor) -> Result<(), MotorError> {
            Ok(())
        }
    }

    fn mock_driver() -> Arc<Mutex<dyn MotorDriver>> {
        Arc::new(Mutex::new(TestMockDriver))
    }

    #[tokio::test]
    async fn motor_controller_initializes() {
        let driver = mock_driver();
        let ctrl = MotorController::new(driver, -1.0, 1.0, 0.8).unwrap();
        assert!((ctrl.max_speed() - 0.8).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn drive_respects_max_speed() {
        let driver = mock_driver();
        let ctrl = MotorController::new(driver, 1.0, 1.0, 0.8).unwrap();

        // Should not error — speed is clamped internally
        ctrl.drive(1.0, 1.0).await.unwrap();
        ctrl.shutdown().await;
    }

    #[tokio::test]
    async fn stop_succeeds() {
        let driver = mock_driver();
        let ctrl = MotorController::new(driver, 1.0, 1.0, 0.8).unwrap();
        ctrl.drive(0.6, 0.6).await.unwrap();
        ctrl.stop().await.unwrap();
        ctrl.shutdown().await;
    }

    #[tokio::test]
    async fn max_speed_clamped_to_valid_range() {
        let driver = mock_driver();
        let ctrl = MotorController::new(driver, 1.0, 1.0, 2.0).unwrap();
        assert!((ctrl.max_speed() - 1.0).abs() < f32::EPSILON);

        // Also clamp below MIN_SPEED
        let driver = mock_driver();
        let ctrl = MotorController::new(driver, 1.0, 1.0, 0.2).unwrap();
        assert!((ctrl.max_speed() - MIN_SPEED).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn dead_zone_prevents_low_speed() {
        // Values below 0.5 become 0.0 (stop)
        assert!((MotorController::apply_dead_zone(0.3, 0.8)).abs() < f32::EPSILON);
        assert!((MotorController::apply_dead_zone(-0.3, 0.8)).abs() < f32::EPSILON);
        assert!((MotorController::apply_dead_zone(0.49, 0.8)).abs() < f32::EPSILON);

        // Values at or above 0.5 pass through (clamped to max)
        assert!((MotorController::apply_dead_zone(0.6, 0.8) - 0.6).abs() < f32::EPSILON);
        assert!((MotorController::apply_dead_zone(-0.7, 0.8) - (-0.7)).abs() < f32::EPSILON);
        assert!((MotorController::apply_dead_zone(0.9, 0.8) - 0.8).abs() < f32::EPSILON);
        assert!((MotorController::apply_dead_zone(-0.9, 0.8) - (-0.8)).abs() < f32::EPSILON);
    }
}
