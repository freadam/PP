using System.Collections.Generic;
using UnityEngine;

namespace GAS
{
    public enum EffectDurationPolicy { Instant, HasDuration, Infinite }

    public enum EffectStackingType { None, AggregateBySource, AggregateByTarget }

    /// <summary>
    /// UGameplayEffect: pure data describing a change to attributes and tags. Never holds runtime state;
    /// apply it by building a <see cref="GameplayEffectSpec"/> (level, source, SetByCaller values) first.
    ///
    /// Instant           -> modifies BaseValue once (damage, healing, costs).
    /// HasDuration       -> modifies CurrentValue while active, grants tags, then expires (slows, stuns, cooldowns).
    /// Infinite          -> like HasDuration but until removed (passive stat boons, regen).
    /// Period > 0        -> the modifiers/executions run as instant changes every Period seconds instead (DoTs, regen).
    /// </summary>
    [CreateAssetMenu(menuName = "GAS/Gameplay Effect", fileName = "GE_New")]
    public class GameplayEffect : ScriptableObject
    {
        [Header("Duration")]
        public EffectDurationPolicy DurationPolicy = EffectDurationPolicy.Instant;
        public GameplayEffectModifierMagnitude DurationMagnitude = GameplayEffectModifierMagnitude.Scalable(0f);
        [Tooltip("Seconds between periodic executions. 0 = not periodic.")]
        public float Period;
        public bool ExecutePeriodicEffectOnApplication = true;

        [Header("Modifiers & Executions")]
        public List<GameplayModifierInfo> Modifiers = new List<GameplayModifierInfo>();
        [SerializeReference] public List<GameplayEffectExecutionCalculation> Executions = new List<GameplayEffectExecutionCalculation>();

        [Header("Tags")]
        [Tooltip("Describe the effect itself (\"Effect.Damage.Magic\"). Not granted to the target.")]
        public GameplayTagContainer AssetTags = new GameplayTagContainer();
        [Tooltip("Granted to the target while the effect is active (duration/infinite only).")]
        public GameplayTagContainer GrantedTags = new GameplayTagContainer();
        [Tooltip("The target must satisfy these for the effect to apply at all.")]
        public GameplayTagRequirements ApplicationTagRequirements = new GameplayTagRequirements();
        [Tooltip("On application, removes active effects whose asset or granted tags match.")]
        public GameplayTagContainer RemoveGameplayEffectsWithTags = new GameplayTagContainer();

        [Header("Stacking")]
        public EffectStackingType StackingType = EffectStackingType.None;
        public int StackLimitCount = 1;
        public bool StackDurationRefreshOnApplication = true;

        [Header("Cues")]
        public GameplayTagContainer GameplayCues = new GameplayTagContainer();

        public bool IsPeriodic => Period > 0f && DurationPolicy != EffectDurationPolicy.Instant;
    }
}
