use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    pub static ref RE_ATTACK: Regex = Regex::new(r"^(?:[^:]+: )*(?P<attacker>.+?) attacks (?P<target>.+?) : (?:\*target concealed: (?P<concealment>\d+)%\* : )?\*(?P<result>hit|miss|critical hit)\*").unwrap();
    pub static ref RE_CONCEALMENT: Regex = Regex::new(r"^(?:[^:]+: )*(?P<attacker>.+?) attacks (?P<target>.+?) : \*target concealed: (?P<concealment>\d+)%\* : \(.+\)").unwrap();
    pub static ref RE_DAMAGE: Regex = Regex::new(r"^(?P<attacker>.+?) damages (?P<target>.+?): (?P<total>\d+) \((?P<breakdown>.+)\)").unwrap();
    pub static ref RE_ABSORB: Regex = Regex::new(r"^(?P<target>.+?) : Damage Immunity absorbs (?P<amount>\d+) point\(s\) of (?P<type>\w+)").unwrap();
    pub static ref RE_ABSORB_RESISTANCE: Regex = Regex::new(r"^(?P<target>.+?) : Damage Resistance absorbs (?P<amount>\d+) damage").unwrap();
    pub static ref RE_ABSORB_REDUCTION: Regex = Regex::new(r"^(?P<target>.+?) : Damage Reduction absorbs (?P<amount>\d+) damage").unwrap();
    pub static ref RE_TIMESTAMP: Regex = Regex::new(r"^\[CHAT WINDOW TEXT\] \[([^\]]+)\]").unwrap();
    pub static ref RE_SPELL_RESIST: Regex = Regex::new(r"^SPELL RESIST: (?P<target>.+?) attempts to resist: (?P<spell>.+?) - Result:\s+(?P<result>FAILED|SUCCESS)").unwrap();
    pub static ref RE_SAVE: Regex = Regex::new(r"^SAVE: (?P<target>.+?) : (?P<save_type>.+?) vs\. (?P<element>.+?) : \*(?P<result>failed|succeeded)\*").unwrap();
    pub static ref RE_CASTING: Regex = Regex::new(r"^(?P<caster>.+?) casting (?P<spell>.+)").unwrap();
    pub static ref RE_CASTS: Regex = Regex::new(r"^(?P<caster>.+?) casts (?P<spell>.+)").unwrap();

    // Player identification regexes
    pub static ref RE_PLAYER_JOIN: Regex = Regex::new(r"^(?P<account>\w+) has joined as a player\.\.").unwrap();
    pub static ref RE_PLAYER_CHAT: Regex = Regex::new(r"^\[(?P<account>\w+)\] (?P<character>[^:]+): \[(?P<chat_type>[^\]]+)\]").unwrap();
    pub static ref RE_PARTY_CHAT: Regex = Regex::new(r"^(?P<character>[^:]+) : \[Party\]").unwrap();
    pub static ref RE_PARTY_JOIN: Regex = Regex::new(r"^(?P<character>.+?) has joined the party\.").unwrap();

    // Rest detection regex
    pub static ref RE_RESTING: Regex = Regex::new(r"^Resting\.$").unwrap();

    // Buff expiration detection regex (matches both "has worn off" and "wore off")
    pub static ref RE_BUFF_EXPIRED: Regex = Regex::new(r"^(?P<spell_name>[^:]+) (?:has worn off|wore off)\.?$").unwrap();

    // Initiative roll detection regex
    pub static ref RE_INITIATIVE: Regex = Regex::new(r"^(?P<character>.+?) : Initiative Roll :").unwrap();

    // Healing detection regex
    pub static ref RE_HEALED: Regex = Regex::new(r"^(?P<character>.+?) : Healed (?P<amount>\d+) hit points?\.").unwrap();
}