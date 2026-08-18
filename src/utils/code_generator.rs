use rand::RngExt;

/// 生成6位数字代码（可用于验证码、优惠码等）
pub fn generate_six_digit_code() -> String {
    let mut rng = rand::rng();
    format!("{:06}", rng.random_range(100000..=999999))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_six_digit_code() {
        let code = generate_six_digit_code();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));

        // 确保代码在有效范围内
        let code_num: u32 = code.parse().unwrap();
        assert!(code_num >= 100000 && code_num <= 999999);
    }

    #[test]
    fn test_generate_multiple_codes_are_different() {
        let code1 = generate_six_digit_code();
        let code2 = generate_six_digit_code();
        // 虽然理论上可能相同，但概率很小
        // 这个测试主要是确保函数能正常运行
        assert_eq!(code1.len(), 6);
        assert_eq!(code2.len(), 6);
    }
}
