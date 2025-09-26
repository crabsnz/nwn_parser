use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    pub static ref RE_ATTACK: Regex = Regex::new(r"^(?:[^:]+: )*(?P<attacker>.+?) attacks (?P<target>.+?) : (?:\*target concealed: (?P<concealment>\d+)%\* : )?\*(?P<result>hit|miss|critical hit)\*").unwrap();
    pub static ref RE_CONCEALMENT: Regex = Regex::new(r"^(?:[^:]+: )*(?P<attacker>.+?) attacks (?P<target>.+?) : \*target concealed: (?P<concealment>\d+)%\* : \(.+\)").unwrap();
    pub static ref RE_DAMAGE: Regex = Regex::new(r"^(?P<attacker>.+?) damages (?P<target>.+?): (?P<total>\d+) \((?P<breakdown>.+)\)").unwrap();
    pub static ref RE_ABSORB: Regex = Regex::new(r"^(?P<target>.+?) : Damage Immunity absorbs (?P<amount>\d+) point\(s\) of (?P<type>\w+)").unwrap();
    pub static ref RE_TIMESTAMP: Regex = Regex::new(r"^\[CHAT WINDOW TEXT\] \[([^\]]+)\]").unwrap();
    pub static ref RE_SPELL_RESIST: Regex = Regex::new(r"^SPELL RESIST: (?P<target>.+?) attempts to resist: (?P<spell>.+?) - Result:\s+(?P<result>FAILED|SUCCESS)").unwrap();
    pub static ref RE_SAVE: Regex = Regex::new(r"^SAVE: (?P<target>.+?) : (?P<save_type>.+?) vs\. (?P<element>.+?) : \*(?P<result>failed|succeeded)\*").unwrap();
    pub static ref RE_CASTING: Regex = Regex::new(r"^(?P<caster>.+?) casting (?P<spell>.+)").unwrap();
    pub static ref RE_CASTS: Regex = Regex::new(r"^(?P<caster>.+?) casts (?P<spell>.+)").unwrap();
}