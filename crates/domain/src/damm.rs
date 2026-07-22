/// Damm check-character algorithm over base 36.
///
/// Requires a **totally anti-symmetric quasigroup of order 36**: a Latin
/// square `∘` where `x∘y = y∘x ⟹ x = y` and, for every prefix value `c`,
/// `(c∘x)∘y = (c∘y)∘x ⟹ x = y`. The second (weak TA) property is what makes
/// the checksum catch adjacent transpositions; the Latin-square property
/// catches every single-character error.
///
/// No crate ships an order-36 table, so we construct one. Take the group
/// `G = Z6 × Z6` (identified with digits `0..36` via `d = 6·x + y`). Its
/// Sylow 2-subgroup is the Klein four-group — non-cyclic — so by Hall–Paige
/// `G` admits a complete mapping, i.e. an **orthomorphism**: a permutation
/// `σ` such that `θ(v) = σ(v) − v` is also a permutation. Then
///
/// ```text
/// x ∘ y  =  σ(x) + y        (componentwise mod 6)
/// ```
///
/// is a quasigroup (rows shift by a constant, columns permute by `σ`), and it
/// is totally anti-symmetric:
///
/// * `x∘y = y∘x` ⟹ `σ(x) − x = σ(y) − y` ⟹ `θ(x) = θ(y)` ⟹ `x = y`;
/// * `(c∘x)∘y = (c∘y)∘x` ⟹ `σ(z+x) + y = σ(z+y) + x` (where `z = σ(c)`)
///   ⟹ `θ(z+x) = θ(z+y)` ⟹ `x = y`.
///
/// We use a *linear* orthomorphism, built via CRT on `Z6 ≅ Z2 × Z3`: on the
/// `Z2²` component multiply by `[[0,1],[1,1]]` (it and its `+I` are invertible
/// over GF(2)); on the `Z3²` component multiply by `2I` (it and `2I − I = I`
/// are invertible over GF(3)). Both `σ` and `σ − id` are then invertible
/// linear maps, hence permutations.
///
/// ⚠️ This table is **frozen forever** the moment the first label prints —
/// changing any entry invalidates every printed check character. Treat it
/// like the FF1 keys: append-only history, never edit. The exhaustive tests
/// below pin the construction down.
pub struct Damm36;

impl Damm36 {
    pub const ORDER: usize = 36;

    const TABLE: [[u8; 36]; 36] = Self::build_table();

    /// The linear orthomorphism σ of Z6×Z6 described above, on digit encoding
    /// `d = 6·x + y`.
    const fn sigma(d: usize) -> usize {
        let (x, y) = (d / 6, d % 6);
        // Z2² component: (x2, y2) ↦ (y2, x2 + y2)
        let (a1, a2) = (y % 2, (x + y) % 2);
        // Z3² component: (x3, y3) ↦ (2·x3, 2·y3)
        let (b1, b2) = ((2 * (x % 3)) % 3, (2 * (y % 3)) % 3);
        // CRT recombination: z ≡ a (mod 2), z ≡ b (mod 3) ⟺ z = 3a + 4b (mod 6)
        let nx = (3 * a1 + 4 * b1) % 6;
        let ny = (3 * a2 + 4 * b2) % 6;
        6 * nx + ny
    }

    const fn build_table() -> [[u8; 36]; 36] {
        let mut t = [[0u8; 36]; 36];
        let mut x = 0;
        while x < 36 {
            let s = Self::sigma(x);
            let mut y = 0;
            while y < 36 {
                t[x][y] = (6 * ((s / 6 + y / 6) % 6) + (s % 6 + y % 6) % 6) as u8;
                y += 1;
            }
            x += 1;
        }
        t
    }

    /// Fold a digit sequence through the quasigroup, starting from 0.
    fn interim(digits: impl IntoIterator<Item = u8>) -> u8 {
        digits
            .into_iter()
            .fold(0u8, |i, d| Self::TABLE[i as usize][d as usize])
    }

    /// The check digit for a payload: the unique `c` such that appending it
    /// makes the whole sequence [`Self::validate`].
    pub fn check_digit(digits: impl IntoIterator<Item = u8>) -> u8 {
        let row = &Self::TABLE[Self::interim(digits) as usize];
        row.iter()
            .position(|&v| v == 0)
            .expect("every row of a Latin square contains 0") as u8
    }

    /// Validate a full sequence (payload + trailing check digit).
    pub fn validate(digits: impl IntoIterator<Item = u8>) -> bool {
        Self::interim(digits) == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every row and every column is a permutation of 0..36.
    #[test]
    fn table_is_latin_square() {
        let full = |it: &mut dyn Iterator<Item = u8>| {
            let mut seen = [false; 36];
            it.for_each(|v| seen[v as usize] = true);
            seen.iter().all(|&s| s)
        };
        (0..36).for_each(|x| {
            assert!(full(&mut (0..36).map(|y| Damm36::TABLE[x][y])), "row {x}");
            assert!(full(&mut (0..36).map(|y| Damm36::TABLE[y][x])), "col {x}");
        });
    }

    /// x∘y = y∘x only when x = y.
    #[test]
    fn table_is_anti_symmetric() {
        (0..36)
            .flat_map(|x| (0..36).map(move |y| (x, y)))
            .filter(|(x, y)| x != y)
            .for_each(|(x, y)| {
                assert_ne!(Damm36::TABLE[x][y], Damm36::TABLE[y][x], "({x},{y})");
            });
    }

    /// (c∘x)∘y = (c∘y)∘x only when x = y — the property behind adjacent-
    /// transposition detection, checked over all 36³ triples.
    #[test]
    fn table_is_totally_anti_symmetric() {
        (0..36)
            .flat_map(|c| (0..36).flat_map(move |x| (0..36).map(move |y| (c, x, y))))
            .filter(|(_, x, y)| x != y)
            .for_each(|(c, x, y)| {
                let cx = Damm36::TABLE[c][x] as usize;
                let cy = Damm36::TABLE[c][y] as usize;
                assert_ne!(Damm36::TABLE[cx][y], Damm36::TABLE[cy][x], "({c},{x},{y})");
            });
    }

    /// The exact table is load-bearing once labels print. Pin a fingerprint so
    /// an accidental change to the construction fails loudly.
    #[test]
    fn table_is_frozen() {
        let flat: Vec<u8> = (0..36)
            .flat_map(|x| Damm36::TABLE[x].iter().copied())
            .collect();
        // FNV-1a over the flattened table.
        let fp = flat
            .iter()
            .fold(0xcbf29ce484222325u64, |h, &b| {
                (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
            });
        assert_eq!(fp, FROZEN_FINGERPRINT);
    }

    /// Recorded 2026-07-22 once the construction passed the property tests.
    /// Never update this value: printed labels depend on the exact table.
    const FROZEN_FINGERPRINT: u64 = 2805603532958343269;

    #[test]
    fn check_digit_round_trips() {
        let payload = [10u8, 20, 30, 0, 1, 35];
        let check = Damm36::check_digit(payload);
        assert!(Damm36::validate(payload.iter().copied().chain([check])));
    }

    #[test]
    fn detects_all_single_digit_errors_and_adjacent_transpositions() {
        // A spread of payloads, not just one.
        let payloads: Vec<[u8; 6]> = (0..50)
            .map(|k| std::array::from_fn(|i| ((k * 7 + i * 11 + k * i) % 36) as u8))
            .collect();
        payloads.iter().for_each(|p| {
            let code: Vec<u8> = p.iter().copied().chain([Damm36::check_digit(p.iter().copied())]).collect();
            // Single-character errors, every position, every wrong value.
            (0..code.len())
                .flat_map(|i| (0..36u8).map(move |v| (i, v)))
                .filter(|&(i, v)| code[i] != v)
                .for_each(|(i, v)| {
                    let mut bad = code.clone();
                    bad[i] = v;
                    assert!(!Damm36::validate(bad), "mutation at {i} -> {v} undetected");
                });
            // Adjacent transpositions of unequal digits, every position.
            (0..code.len() - 1)
                .filter(|&i| code[i] != code[i + 1])
                .for_each(|i| {
                    let mut bad = code.clone();
                    bad.swap(i, i + 1);
                    assert!(!Damm36::validate(bad), "transposition at {i} undetected");
                });
        });
    }
}
