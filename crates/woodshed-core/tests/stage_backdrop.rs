use woodshed_core::stage_scene::{stage_scene, StageSceneOptions};
use woodshedding::rehearsal::{LoopMode, Set};

#[test]
fn the_stage_floor_is_portable_environment_data() {
    let set = Set::from_cards([], LoopMode::Off);
    let stage = stage_scene(&set, &StageSceneOptions::default());
    let backdrop = stage
        .snapshot
        .active_backdrops_in_order()
        .into_iter()
        .next()
        .expect("stage floor")
        .1;

    assert_eq!(backdrop.kind, "woodshed:stage-floor");
    assert!(backdrop.visible);
    assert!(!backdrop.collidable);

    let wire = serde_json::to_string(&stage.snapshot).expect("serialize stage");
    let remote: scenotime::SceneSnapshot = serde_json::from_str(&wire).expect("remote scene");
    assert_eq!(remote.active_backdrops_in_order().len(), 1);
    assert_eq!(remote.active_backdrops_in_order()[0].1, backdrop);
}
