//! Whether a GGUF fits this phone.
//!
//! Modelled on the on-device size check: a model fits when total RAM is at
//! least its floor and `size * 1.25 + headroom` is still under total RAM.
//! Headroom is 1.5 GiB plus the resident Moonshine Base and Piper lessac
//! footprints. `tight` uses 85% of the file and a floor one gigabyte lower.
//! `fits` ranks above `tight`, which ranks above `unknown`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    Fits,
    Tight,
    Unknown,
    No,
}

impl Fit {
    pub fn as_str(self) -> &'static str {
        match self {
            Fit::Fits => "fits",
            Fit::Tight => "tight",
            Fit::Unknown => "unknown",
            Fit::No => "no",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Fit::Fits => 3,
            Fit::Tight => 2,
            Fit::Unknown => 1,
            Fit::No => 0,
        }
    }
}

/// 1.5 GiB left for the system and the KV cache, on top of speech and voice.
pub const EXTRA_HEADROOM_BYTES: u64 = 1_610_612_736;
/// Published Moonshine Base English int8 archive. It stays loaded beside the chat model.
pub const MOONSHINE_FOOTPRINT: u64 = 250_807_309;
/// Published Piper en_US lessac low archive.
pub const PIPER_FOOTPRINT: u64 = 67_097_098;

pub fn resident_headroom() -> u64 {
    EXTRA_HEADROOM_BYTES + MOONSHINE_FOOTPRINT + PIPER_FOOTPRINT
}

/// Nearest GiB, so a phone that reports a little under 8 GiB still counts as 8.
pub fn total_gib(total_bytes: u64) -> u64 {
    if total_bytes == 0 {
        return 0;
    }
    let gib = 1024u64 * 1024 * 1024;
    total_bytes.saturating_add(gib / 2) / gib
}

fn scaled_fits(size: u64, numer: u64, denom: u64, total_bytes: u64) -> bool {
    let scaled = size.saturating_mul(numer) / denom;
    let with_runtime = scaled.saturating_mul(5) / 4;
    with_runtime.saturating_add(resident_headroom()) < total_bytes
}

pub fn classify(total_bytes: u64, size_bytes: u64, min_ram_gb: u32) -> Fit {
    if total_bytes == 0 || size_bytes == 0 || min_ram_gb == 0 {
        return Fit::Unknown;
    }
    let total_gb = total_gib(total_bytes);
    let floor = u64::from(min_ram_gb);
    if total_gb >= floor && scaled_fits(size_bytes, 1, 1, total_bytes) {
        return Fit::Fits;
    }
    let tight_floor = floor.saturating_sub(1);
    if total_gb >= tight_floor && scaled_fits(size_bytes, 85, 100, total_bytes) {
        return Fit::Tight;
    }
    Fit::No
}

#[derive(Clone, Copy, Debug)]
pub struct Candidate<'a> {
    pub id: &'a str,
    pub size_bytes: u64,
    pub min_ram_gb: u32,
    pub strength: u32,
}

/// Strongest model in the best rank that is not `no`.
pub fn recommend<'a>(candidates: &[Candidate<'a>], total_bytes: u64) -> Option<&'a str> {
    let mut best: Option<(Fit, u32, &'a str)> = None;
    for candidate in candidates {
        let fit = classify(total_bytes, candidate.size_bytes, candidate.min_ram_gb);
        if matches!(fit, Fit::No) {
            continue;
        }
        let replace = match best {
            None => true,
            Some((rank, strength, _)) => {
                fit.rank() > rank.rank()
                    || (fit.rank() == rank.rank() && candidate.strength > strength)
            }
        };
        if replace {
            best = Some((fit, candidate.strength, candidate.id));
        }
    }
    best.map(|(_, _, id)| id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadGate {
    Allow,
    Warn,
    BlockStorage,
    BlockFit,
}

pub fn download_gate(fit: Fit, free_storage: u64, bytes: u64) -> DownloadGate {
    if bytes > 0 && free_storage > 0 && free_storage < bytes {
        return DownloadGate::BlockStorage;
    }
    match fit {
        Fit::No => DownloadGate::BlockFit,
        Fit::Tight => DownloadGate::Warn,
        Fit::Fits | Fit::Unknown => DownloadGate::Allow,
    }
}

pub fn gate_message(gate: DownloadGate) -> Option<&'static str> {
    match gate {
        DownloadGate::Allow => None,
        DownloadGate::Warn => Some(
            "This model is tight on this phone. Download only if you want to try it.",
        ),
        DownloadGate::BlockStorage => Some("Not enough free storage for this model."),
        DownloadGate::BlockFit => Some("This model does not fit this phone."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;
    const SMOL: u64 = 144_811_360;
    const QWEN08: u64 = 811_843_840;
    const QWEN2: u64 = 1_213_300_992;
    const GEMMA4: u64 = 2_620_370_976;
    const ATOMIC: u64 = 2_708_804_608;

    fn phone(gib: u64) -> u64 {
        gib * GIB
    }

    #[test]
    fn eight_gig_phone_fits_the_4b_and_recommends_it_over_gemma() {
        assert_eq!(classify(phone(8), ATOMIC, 8), Fit::Fits);
        assert_eq!(classify(phone(8), GEMMA4, 8), Fit::Fits);
        assert_eq!(classify(phone(8), QWEN2, 3), Fit::Fits);
        let picked = recommend(
            &[
                Candidate { id: "gemma4", size_bytes: GEMMA4, min_ram_gb: 8, strength: 8 },
                Candidate { id: "atomic", size_bytes: ATOMIC, min_ram_gb: 8, strength: 9 },
                Candidate { id: "qwen2", size_bytes: QWEN2, min_ram_gb: 3, strength: 7 },
            ],
            phone(8),
        );
        assert_eq!(picked, Some("atomic"));
    }

    #[test]
    fn two_gig_phone_keeps_smollm_and_refuses_the_larger_qwen() {
        assert_eq!(classify(phone(2), SMOL, 2), Fit::Fits);
        assert_eq!(classify(phone(2), QWEN08, 2), Fit::No);
        assert_eq!(
            recommend(
                &[
                    Candidate { id: "smol", size_bytes: SMOL, min_ram_gb: 2, strength: 1 },
                    Candidate { id: "qwen08", size_bytes: QWEN08, min_ram_gb: 2, strength: 4 },
                ],
                phone(2),
            ),
            Some("smol")
        );
    }

    #[test]
    fn three_gig_phone_marks_qwen_2b_tight_and_ranks_fits_first() {
        assert_eq!(classify(phone(3), QWEN2, 3), Fit::Tight);
        assert_eq!(classify(phone(3), QWEN08, 2), Fit::Fits);
        assert_eq!(
            recommend(
                &[
                    Candidate { id: "qwen2", size_bytes: QWEN2, min_ram_gb: 3, strength: 7 },
                    Candidate { id: "qwen08", size_bytes: QWEN08, min_ram_gb: 2, strength: 4 },
                ],
                phone(3),
            ),
            Some("qwen08")
        );
    }

    #[test]
    fn missing_probe_is_unknown_and_still_a_candidate() {
        assert_eq!(classify(0, ATOMIC, 8), Fit::Unknown);
        assert_eq!(Fit::Fits.as_str(), "fits");
        assert_eq!(
            recommend(
                &[Candidate { id: "atomic", size_bytes: ATOMIC, min_ram_gb: 8, strength: 9 }],
                0,
            ),
            Some("atomic")
        );
    }

    #[test]
    fn download_blocks_a_poor_fit_or_a_full_disk_and_warns_when_tight() {
        assert_eq!(download_gate(Fit::Fits, 1_000, ATOMIC), DownloadGate::BlockStorage);
        assert_eq!(download_gate(Fit::No, phone(64), ATOMIC), DownloadGate::BlockFit);
        assert_eq!(download_gate(Fit::Tight, phone(64), QWEN2), DownloadGate::Warn);
        assert_eq!(download_gate(Fit::Unknown, 0, QWEN2), DownloadGate::Allow);
        assert!(gate_message(DownloadGate::BlockStorage).unwrap().contains("storage"));
        assert!(gate_message(DownloadGate::Warn).unwrap().contains("tight"));
    }
}
