use regex::Regex;

fn main() {
    // Test the regex patterns
    let attack_regex = Regex::new(r"^(?:[^:]+: )*(?P<attacker>.+?) attacks (?P<target>.+?) : (?:\*target concealed: (?P<concealment>\d+)%\* : )?\*(?P<result>hit|miss|critical hit)\*").unwrap();
    let damage_regex = Regex::new(r"^(?P<attacker>.+?) damages (?P<target>.+?): (?P<total>\d+) \((?P<breakdown>.+)\)").unwrap();
    
    let test_lines = vec![
        "DanK Divine attacks 10 AC DUMMY - DPS TEST : *hit* : (6 + 21 = 27)",
        "DanK Divine attacks 10 AC DUMMY - DPS TEST : *hit* : (8 + 16 = 24)",
        "DanK Divine damages 10 AC DUMMY - DPS TEST: 4 (4 Physical)",
        "DanK Divine damages 10 AC DUMMY - DPS TEST: 5 (5 Physical)",
    ];
    
    for line in test_lines {
        println!("Testing: {}", line);
        if let Some(caps) = attack_regex.captures(line) {
            println!("  ATTACK: {} -> {} ({})", 
                     &caps["attacker"], &caps["target"], &caps["result"]);
        } else if let Some(caps) = damage_regex.captures(line) {
            println!("  DAMAGE: {} -> {} for {} ({})", 
                     &caps["attacker"], &caps["target"], &caps["total"], &caps["breakdown"]);
        } else {
            println!("  NO MATCH!");
        }
    }
}
