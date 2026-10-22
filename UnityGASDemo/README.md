# GAS Arena — Unity prototype

A small top-down action roguelite demo (Ravenswatch, Curse of the Dead Gods, Ruins of Albion) with
League-of-Legends-style hero abilities. It is built on a C# port of the main ideas in Unreal Engine's
**Gameplay Ability System (GAS)**.

Everything is generated at runtime from code: primitives, programmer art and IMGUI. You need no assets,
prefabs or project setup.

## Run it

1. Open `UnityGASDemo/` in Unity **6000.0+** (Unity 2022.3 LTS works too). Unity Hub lets you choose the
   editor version.
2. Choose **Tools → GAS Demo → Create Demo Scene**, then press **Play**.
   *(Or: add an empty GameObject to any empty scene, attach `DemoBootstrap`, then press Play.)*

To drop the demo into an existing project, copy `Assets/GASDemo/`. It works with the built-in pipeline and
with URP, and with the legacy Input Manager or the Input System package.

### Ability sandbox

Choose **Tools → GAS Demo → Create Ability Sandbox Scene**, then press Play. You get the hero and a range of
stationary capsule dummies. There are no waves and nothing attacks you. Each dummy shows its last hit, total
damage, hit count and DPS, and heals back to full after 3 seconds without taking damage.

| Dummy | Use it to test |
|---|---|
| Line A/B/C | Combo cone width, Mystic Shot, Tumble-empowered hits |
| Pack 1–5 (cluster) | Frost Field ticks and slow, Shockwave stun |
| Armored (150) | Physical vs. magic damage against armor |
| Fragile (60 HP, respawns) | On-kill effects (Bloodthirst) |
| Far | Mystic Shot range from the start position |

Dummies are `Anchored`: knockback and pushes don't move them.

Cheats: **F2** refill and reset cooldowns · **F3** toggle free casts · **F4 / F5** toggle the Ignite / Bloodthirst
passives · **F6** rank up all abilities (wraps back to 1) · **F7** stun every dummy for 2s · **F1** GAS debug panel.

### Controls

| Input | Ability | Inspired by |
|---|---|---|
| WASD / mouse | Move / aim | Hades, Ravenswatch |
| **LMB** (hold) | **Blade Combo**: 3 hits, wide finisher with knockback | Riven Q, Ravenswatch |
| **RMB** | **Mystic Shot**: skillshot. On hit, all cooldowns drop by 1.5s and you get mana back | Ezreal Q + passive |
| **Q** | **Frost Field**: ground zone that deals damage over time and slows | Anivia R, Brand W |
| **Space** | **Tumble**: dash with i-frames that empowers your next basic attack | Vayne Q |
| **R** | **Shockwave**: channel, then an AoE slam that knocks back and stuns | Amumu R, Malphite R |
| **1 / 2 / 3** | Pick a boon after a wave (stats, new passives, ability ranks) | Curse of the Dead Gods relics |
| **F1** | GAS debug panel: attributes, owned tags, active effects, abilities | UE `showdebug abilitysystem` |

There are three enemy types. Each one uses the same GAS as the hero:
- **Grunt**: telegraphed cone swipe.
- **Brute**: telegraphed slam with knockback.
- **Caster**: telegraphed bolt that keeps its distance.

If you stun an enemy during its wind-up, the attack is cancelled.

---

## Architecture

```
Assets/GASDemo/
  GAS/Runtime/         ← the framework (asmdef GAS.Runtime). No game code; only depends on UnityEngine.
    Tags/              GameplayTag, GameplayTagContainer, GameplayTagCountContainer, GameplayTagRequirements
    Attributes/        GameplayAttribute, GameplayAttributeData, AttributeSet
    Effects/           GameplayEffect (SO), GameplayEffectSpec, ActiveGameplayEffectsContainer,
                       modifiers / magnitudes / ScalableFloat, GameplayEffectExecutionCalculation
    Abilities/         GameplayAbility (SO), GameplayAbilitySpec, AbilityTask + standard tasks
    Cues/              GameplayCueManager, GameplayCueNotify, GameplayCueNotifyActor
    AbilitySystemComponent.cs, AbilitySystemGlobals.cs
  Game/                ← the demo (asmdef GASDemo.Game), built only on the GAS public API
    Core/              GameTags (native tags), CharacterAttributeSet, DamageExecution, Targeting
    Abilities/         DemoGameplayAbility base + hero / enemy / passive abilities, AbilityTask_Dash
    Characters/        CharacterBase (avatar), HeroInput, EnemyBrain (controllers)
    Content/           DemoContent (all effects and abilities authored in code), Boons
    World/             Projectile, GroundZone, cues, telegraphs, camera, wave director, Fx
    UI/                DemoHUD
  Editor/              menu: create scene, export content as .asset files
```

### UE GAS → this project

| Unreal | Here | Notes |
|---|---|---|
| `UAbilitySystemComponent` | `AbilitySystemComponent` (MonoBehaviour) | The one entry point for attributes, tags, effects, abilities, events and cues. |
| `FGameplayTag` / container | `GameplayTag`, `GameplayTagContainer` | Hierarchical: `State.Debuff.Stun` matches `State.Debuff`. |
| Owned tags (`FGameplayTagCountContainer`) | `GameplayTagCountContainer` | Ref-counted. Parents are counted implicitly. Fires events when a tag is added or removed. |
| `UAttributeSet` + `FGameplayAttributeData` | `AttributeSet`, `GameplayAttributeData` | Each attribute has a base and a current value, plus `PreAttributeChange`, `PostGameplayEffectExecute` and so on. |
| Meta attributes | `CharacterAttributeSet.Damage` | Damage → mitigation → Health, death and `Event.Kill` all happen in one place. |
| `UGameplayEffect` | `GameplayEffect` (ScriptableObject) | Instant, HasDuration or Infinite. Supports period, modifiers, executions, granted tags, application requirements, remove-with-tags, stacking and cues. |
| `FGameplayEffectSpec` | `GameplayEffectSpec` | Holds the level, context and SetByCaller magnitudes, plus dynamic granted tags and a duration override. |
| Aggregator | `ActiveGameplayEffectsContainer.Aggregate` | `(Base + ΣAdd) × (1 + Σ(Mul−1))`, or Override. |
| `UGameplayEffectExecutionCalculation` | `GameplayEffectExecutionCalculation` | Example: `DamageExecution` (AD/AP ratios, armor, stacks). |
| `UGameplayAbility` (instanced per actor) | `GameplayAbility` (ScriptableObject, cloned per ASC on `GiveAbility`) | Has tags for cancel, block, activation-owned, required and blocked. Also has cost and cooldown effects, triggers, and activate-on-granted. |
| `CommitAbility`, cost/cooldown GEs | `CommitAbility`, `CostEffect`, `CooldownEffect` + `CooldownDuration` + `CooldownTags` | One shared `GE_Cooldown` serves every ability (the SetByCaller-style dynamic cooldown pattern). |
| `FGameplayAbilitySpec` | `GameplayAbilitySpec` | Holds the level, the input ID and the runtime instance. |
| `UAbilityTask` | `AbilityTask` + `WaitDelay`, `WaitInputRelease`, `WaitGameplayEvent`, `WaitGameplayTag`, `Dash` | Tasks are owned by one activation. When the ability ends, its tasks end. |
| Gameplay events / triggers | `HandleGameplayEvent`, `AbilityTriggers` | Passives such as Arcane Echo, Ignite and Bloodthirst are event-triggered abilities. |
| `UGameplayCueManager` / notifies | `GameplayCueManager`, `GameplayCueNotify`, `GameplayCueNotifyActor` | Cosmetics only. Gameplay code fires `GameplayCue.*` tags. |
| `IAbilitySystemInterface`, blueprint library | `IAbilitySystemInterface`, `AbilitySystemGlobals` | |

### How a hit flows through the system

The example below is Mystic Shot hitting a stunned-but-not-invulnerable Grunt:

1. `HeroInput` calls `asc.AbilityLocalInputPressed(Secondary)`, and the ASC activates the bound spec.
2. `GA_FireProjectile.ActivateAbility` calls `CommitAbility()`. That applies `GE_Cost_Mana` (SetByCaller −25) and
   `GE_Cooldown` (4s, granting `Cooldown.Skill.MysticShot`).
3. The ability builds a `GE_Damage_Magic` spec (`Data.Damage`, `Data.Ratio.AD`, `Data.Ratio.AP`) and hands it to a `Projectile`.
4. The projectile hits and applies the spec. On the target, `ApplicationTagRequirements` rejects the hit if the target
   owns `State.Invulnerable`. If not, `DamageExecution` reads the source's AttackPower/AbilityPower and the target's Armor.
5. `CharacterAttributeSet.PostGameplayEffectExecute` turns the `Damage` meta attribute into Health loss. It then sends
   `Event.Damaged` (damage numbers, hit flash) and, if the target died, adds `State.Dead` and sends `Event.Kill`
   to the instigator.
6. The projectile sends `Event.Hit.MysticShot` to the hero. That event triggers `GA_Passive_ArcaneEcho`, which
   shortens every active effect matching `Cooldown` by 1.5s and restores mana. With the Ignite boon, the same
   event also matches `Event.Hit` and applies a stacking periodic burn.
7. Cue tags on the effects (`GameplayCue.Hit.Magic`, `GameplayCue.Status.Burn`) produce the visuals.

No step needs to know about the step before or after it. They only share tags and specs.

### Adding an ability

1. Subclass `DemoGameplayAbility` and implement `ActivateAbility`: commit, run tasks, apply specs, then `EndAbility`.
2. Configure it in `DemoContent`: tags, cost, cooldown and effects. You can also export the content
   (**Tools → GAS Demo → Export Content As Assets**) and tune the generated `.asset` files in the inspector.
3. Grant it with `asc.GiveAbility(ability, level, (int)inputId)`.

Rules worth keeping:
- Change state only through the ASC: effects for numbers, tags for state.
- Abilities aim through `CharacterBase.AimPoint`, so the same ability works for AI.

### Deliberate simplifications (prototype scope)

- No networking, prediction or replication. Those are UE GAS's hardest parts and are out of scope here.
- Targeting uses a character registry with distance checks instead of physics or target actors.
- Modifier magnitudes are snapshotted on application. Periodic ticks re-evaluate.
- No ongoing tag requirements (effect inhibition), no immunity queries, no granted abilities on effects.
- Each ASC is both owner and avatar (no PlayerState split).
