//! The DEPS reader and resolver and the work/span model of a ninja/siso build live in fw (`fw-depot`, run as `fw depot`); the paths
//! `ay_build::deps` and `ay_build::siso` stay.

#[cfg(feature = "deps")]
pub use fw_depot::deps;
#[cfg(feature = "siso")]
pub use fw_depot::siso;

#[cfg(all(test, feature = "full"))]
mod tests {
    #[test]
    fn deps_and_the_build_model_come_from_fw() {
        let deps = super::deps::Deps::parse("vars = {'a': 'b'}\ndeps = {}\n").expect("a DEPS file");
        assert_eq!(deps.vars.len(), 1);
        let dag = super::siso::Dag::new(2);
        assert_eq!(dag.len(), 2);
    }
}
