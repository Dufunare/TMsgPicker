//! Explicit boundary evidence is separate from scope completion.
//! TDLib media_album_id is a grouping key, NOT an authoritative member count.
use crate::core::{CompletenessLevel, PostCompleteness, PostKind};

#[derive(Clone,Copy,Debug,Default)]
pub struct BoundaryEvidence {
    pub newer_edge:bool,
    pub older_edge:bool,
    /// Only set when the scanner has *positive* evidence of omitted members.
    /// Merely observing fewer than ten Album members never satisfies this.
    pub known_missing_members:bool,
}
pub fn classify(kind:&PostKind,evidence:BoundaryEvidence)->PostCompleteness {
    let (level,reason)=if evidence.known_missing_members {
        (CompletenessLevel::Truncated,"known_members_omitted")
    } else if *kind==PostKind::Single {
        (CompletenessLevel::Verified,"single_message")
    } else if evidence.newer_edge&&evidence.older_edge {
        (CompletenessLevel::Assumed,"contiguous_boundaries_observed")
    } else if !evidence.newer_edge {
        (CompletenessLevel::Unknown,"newer_boundary_not_observed")
    } else {
        (CompletenessLevel::Unknown,"older_boundary_not_observed")
    };
    PostCompleteness{level,reason:reason.into()}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unproven_album_never_becomes_verified() {
        let sure=BoundaryEvidence{newer_edge:true,older_edge:true,..Default::default()};
        assert_eq!(classify(&PostKind::Album,sure).level,CompletenessLevel::Assumed);
        assert_eq!(classify(&PostKind::Single,sure).level,CompletenessLevel::Verified);
        assert_eq!(classify(&PostKind::Album,BoundaryEvidence{older_edge:false,..sure}).level,CompletenessLevel::Unknown);
        assert_eq!(classify(&PostKind::Album,BoundaryEvidence{known_missing_members:true,..sure}).level,CompletenessLevel::Truncated);
    }
}
