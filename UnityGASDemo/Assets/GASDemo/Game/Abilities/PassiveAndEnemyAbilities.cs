using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// Passive (Ezreal's Rising Spell Force, reshaped): whenever Mystic Shot hits, refund cooldown on every ability
    /// and restore mana. Triggered by the "Event.Hit.MysticShot" gameplay event — the projectile never knows this exists.
    /// </summary>
    public class GA_Passive_ArcaneEcho : DemoGameplayAbility
    {
        public GameplayEffect RestoreManaEffect;
        public float CooldownRefund = 1.5f;
        public float ManaRestored = 12f;

        protected override void ActivateAbility(GameplayEventData triggerEventData)
        {
            AbilitySystem.ActiveEffects.ModifyTimeRemaining(GameplayTagContainer.Of(GameTags.Cooldown), -CooldownRefund);

            if (RestoreManaEffect != null)
            {
                var spec = MakeOutgoingGameplayEffectSpec(RestoreManaEffect);
                spec.SetSetByCallerMagnitude(GameTags.Data_Mana, ManaRestored);
                AbilitySystem.ApplyGameplayEffectSpecToSelf(spec);
            }

            FloatingText.Spawn(AvatarActor.transform.position + Vector3.up * 2.6f, "Echo", new Color(0.5f, 0.85f, 1f));
            EndAbility();
        }
    }

    /// <summary>Boon passive: every hit ignites the target with a stacking burn (periodic effect, AggregateBySource).</summary>
    public class GA_Passive_Ignite : DemoGameplayAbility
    {
        public GameplayEffect BurnEffect;
        public float DamagePerTick = 4f;
        public float APRatioPerTick = 0.05f;

        protected override void ActivateAbility(GameplayEventData triggerEventData)
        {
            var target = AbilitySystemGlobals.GetAbilitySystemComponentFromActor(triggerEventData?.Target);
            if (target != null && BurnEffect != null)
            {
                var spec = MakeDamageSpec(BurnEffect, DamagePerTick, 0f, APRatioPerTick);
                ApplyGameplayEffectSpecToTarget(spec, target);
            }
            EndAbility();
        }
    }

    /// <summary>Boon passive: heal a percentage of max health on every kill (Event.Kill trigger).</summary>
    public class GA_Passive_Bloodthirst : DemoGameplayAbility
    {
        public GameplayEffect HealEffect;
        public float MaxHealthFraction = 0.06f;

        protected override void ActivateAbility(GameplayEventData triggerEventData)
        {
            if (HealEffect != null)
            {
                float amount = AbilitySystem.GetNumericAttribute(CharacterAttributeSet.MaxHealth) * MaxHealthFraction;
                var spec = MakeOutgoingGameplayEffectSpec(HealEffect);
                spec.SetSetByCallerMagnitude(GameTags.Data_Heal, amount);
                AbilitySystem.ApplyGameplayEffectSpecToSelf(spec);
                FloatingText.Spawn(AvatarActor.transform.position + Vector3.up * 2.6f, "+" + Mathf.RoundToInt(amount), new Color(0.4f, 1f, 0.5f));
            }
            EndAbility();
        }
    }

    /// <summary>
    /// Enemy melee/slam with a readable telegraph: lock aim, show the danger zone filling up, then strike.
    /// Stunning the enemy mid-wind-up cancels the ability, and OnEndAbility removes the telegraph — the
    /// "interrupt" mechanic falls out of tags with no extra code.
    /// </summary>
    public class GA_TelegraphedStrike : DemoGameplayAbility
    {
        public enum StrikeShape { Cone, Circle }

        public GameplayEffect DamageEffect;
        public StrikeShape Shape = StrikeShape.Cone;
        public float Range = 2.4f;
        public float Angle = 90f;
        public float WindUp = 0.6f;
        public float Recovery = 0.4f;
        public ScalableFloat BaseDamage = new ScalableFloat(12f, 3f);
        public float ADRatio = 1f;
        public float Knockback;
        public Color TelegraphColor = new Color(1f, 0.2f, 0.15f, 0.3f);

        GameObject telegraph;

        protected override void ActivateAbility(GameplayEventData triggerEventData)
        {
            if (!CommitAbility()) { EndAbility(true); return; }

            var avatar = Avatar;
            avatar.FaceTowards(avatar.AimPoint);
            float angle = Shape == StrikeShape.Circle ? 360f : Angle;
            telegraph = Telegraphs.Create(avatar.transform.position, avatar.transform.rotation, Range, angle, WindUp, TelegraphColor);

            var wait = AbilityTask_WaitDelay.WaitDelay(this, WindUp);
            wait.OnFinish += Strike;
            wait.ReadyForActivation();
        }

        void Strike()
        {
            if (telegraph != null) Destroy(telegraph);

            var avatar = Avatar;
            var origin = avatar.transform.position;
            var targets = Shape == StrikeShape.Circle
                ? Targeting.HostilesInRadius(OwnerTeam, origin, Range)
                : Targeting.HostilesInCone(OwnerTeam, origin, avatar.transform.forward, Range, Angle);

            var spec = MakeDamageSpec(DamageEffect, BaseDamage.GetValueAtLevel(GetAbilityLevel()), ADRatio, 0f);
            foreach (var target in targets)
            {
                ApplyToTarget(spec, target);
                if (Knockback > 0f) target.ApplyKnockback(target.transform.position - origin, Knockback);
            }

            if (Shape == StrikeShape.Circle)
            {
                Fx.GroundRing(origin, TelegraphColor.WithAlpha(0.8f), Range, 0.25f);
                TopDownCamera.Shake(0.15f, 0.2f);
            }
            else
            {
                Fx.Burst(origin + avatar.transform.forward * Range * 0.6f + Vector3.up, TelegraphColor.WithAlpha(0.6f), 0.4f, Range, 0.15f);
            }

            var recover = AbilityTask_WaitDelay.WaitDelay(this, Recovery);
            recover.OnFinish += () => EndAbility();
            recover.ReadyForActivation();
        }

        protected override void OnEndAbility(bool wasCancelled)
        {
            if (telegraph != null) Destroy(telegraph);
        }
    }
}
