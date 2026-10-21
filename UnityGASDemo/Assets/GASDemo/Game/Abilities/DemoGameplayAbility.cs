using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// Project-level ability base (like a game's UMyGameplayAbility): adds an input binding, mana cost as a
    /// SetByCaller on a shared cost effect, ability-haste-scaled cooldowns, and damage helpers.
    /// </summary>
    public abstract class DemoGameplayAbility : GameplayAbility
    {
        [Header("Demo")]
        public string DisplayName = "Ability";
        [TextArea] public string Description = "";
        public AbilityInputID InputID = AbilityInputID.None;
        public ScalableFloat ManaCost;
        public int MaxLevel = 5;

        protected CharacterBase Avatar => AvatarActor != null ? AvatarActor.GetComponent<CharacterBase>() : null;
        protected Team OwnerTeam => Avatar != null ? Avatar.Team : Team.Monsters;

        public float GetManaCost() => ManaCost.GetValueAtLevel(GetAbilityLevel());

        protected override GameplayEffectSpec MakeCostSpec()
        {
            if (CostEffect == null || GetManaCost() <= 0f) return null;
            var spec = MakeOutgoingGameplayEffectSpec(CostEffect);
            spec.SetSetByCallerMagnitude(GameTags.Data_Cost, -GetManaCost());
            return spec;
        }

        protected override float GetCooldownDuration()
        {
            float haste = AbilitySystem.GetNumericAttribute(CharacterAttributeSet.AbilityHaste);
            return base.GetCooldownDuration() * 100f / (100f + Mathf.Max(0f, haste));
        }

        /// <summary>A damage spec: base + AD/AP ratios, resolved later by <see cref="DamageExecution"/> on the target.</summary>
        protected GameplayEffectSpec MakeDamageSpec(GameplayEffect damageEffect, float baseDamage, float adRatio, float apRatio)
        {
            var spec = MakeOutgoingGameplayEffectSpec(damageEffect);
            spec.SetSetByCallerMagnitude(GameTags.Data_Damage, baseDamage);
            spec.SetSetByCallerMagnitude(GameTags.Data_ADRatio, adRatio);
            spec.SetSetByCallerMagnitude(GameTags.Data_APRatio, apRatio);
            return spec;
        }

        /// <summary>Apply a spec to a character and, if it landed, tell our own ASC with a hit event (drives on-hit passives).</summary>
        protected bool ApplyToTarget(GameplayEffectSpec spec, CharacterBase target, GameplayTag hitEvent = default)
        {
            spec.Context.AddHitLocation(target.transform.position + Vector3.up);
            bool landed = ApplyGameplayEffectSpecToTarget(spec, target.AbilitySystem).WasSuccessfullyApplied;
            if (landed && hitEvent.IsValid)
            {
                AbilitySystem.HandleGameplayEvent(hitEvent, new GameplayEventData
                {
                    Instigator = AvatarActor,
                    Target = target.gameObject,
                    TargetLocation = target.transform.position,
                    HasTargetLocation = true,
                    OptionalObject = this,
                });
            }
            return landed;
        }
    }
}
