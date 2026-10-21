using System.Collections.Generic;
using UnityEngine;

namespace GAS
{
    /// <summary>FGameplayEffectContext: who caused an effect and how.</summary>
    public class GameplayEffectContext
    {
        /// <summary>The ASC that owns the ability/effect that created this (the "source").</summary>
        public AbilitySystemComponent Instigator;
        /// <summary>The physical thing that caused it — a projectile, a ground zone, the avatar itself.</summary>
        public GameObject EffectCauser;
        public GameplayAbility Ability;
        public object SourceObject;
        public Vector3 HitLocation;
        public bool HasHitLocation;

        public GameplayEffectContext Duplicate() => (GameplayEffectContext)MemberwiseClone();

        public void AddHitLocation(Vector3 location)
        {
            HitLocation = location;
            HasHitLocation = true;
        }
    }

    /// <summary>
    /// FGameplayEffectSpec: a GameplayEffect ready to be applied — definition + level + context + SetByCaller data.
    /// One spec can be applied to many targets (an AoE builds one spec and applies it to everyone hit).
    /// </summary>
    public class GameplayEffectSpec
    {
        readonly Dictionary<GameplayTag, float> setByCallerMagnitudes = new Dictionary<GameplayTag, float>();
        float? durationOverride;

        public GameplayEffect Def { get; }
        public float Level { get; set; }
        public GameplayEffectContext Context { get; }

        public readonly GameplayTagContainer DynamicGrantedTags = new GameplayTagContainer();
        public readonly GameplayTagContainer DynamicAssetTags = new GameplayTagContainer();

        public GameplayEffectSpec(GameplayEffect def, GameplayEffectContext context, float level = 1f)
        {
            Def = def;
            Context = context ?? new GameplayEffectContext();
            Level = level;
        }

        public void SetSetByCallerMagnitude(GameplayTag dataTag, float magnitude) => setByCallerMagnitudes[dataTag] = magnitude;

        public float GetSetByCallerMagnitude(GameplayTag dataTag, bool warnIfNotFound = true, float defaultIfNotFound = 0f)
        {
            if (setByCallerMagnitudes.TryGetValue(dataTag, out var v)) return v;
            if (warnIfNotFound)
                Debug.LogWarning($"[GAS] SetByCaller '{dataTag}' not set on spec for '{(Def != null ? Def.name : "null")}'");
            return defaultIfNotFound;
        }

        /// <summary>Override the definition's duration (dynamic cooldowns, ability-level-scaled CC...).</summary>
        public void SetDuration(float seconds) => durationOverride = seconds;

        public float CalculateDuration(AbilitySystemComponent target)
        {
            if (Def.DurationPolicy != EffectDurationPolicy.HasDuration) return 0f;
            return durationOverride ?? Def.DurationMagnitude.Calculate(this, target);
        }

        public GameplayTagContainer GetAllGrantedTags()
        {
            var all = new GameplayTagContainer();
            all.AppendTags(Def.GrantedTags);
            all.AppendTags(DynamicGrantedTags);
            return all;
        }

        public GameplayTagContainer GetAllAssetTags()
        {
            var all = new GameplayTagContainer();
            all.AppendTags(Def.AssetTags);
            all.AppendTags(DynamicAssetTags);
            return all;
        }
    }

    /// <summary>FActiveGameplayEffectHandle</summary>
    public readonly struct ActiveGameplayEffectHandle
    {
        public readonly int Id;

        public ActiveGameplayEffectHandle(int id) { Id = id; }

        public static readonly ActiveGameplayEffectHandle Invalid = new ActiveGameplayEffectHandle(0);
        /// <summary>Instant effects apply successfully but leave nothing active behind.</summary>
        public static readonly ActiveGameplayEffectHandle InstantApplied = new ActiveGameplayEffectHandle(-1);

        public bool WasSuccessfullyApplied => Id != 0;
        public bool IsActiveHandle => Id > 0;

        public override string ToString() => $"GEHandle({Id})";
    }
}
