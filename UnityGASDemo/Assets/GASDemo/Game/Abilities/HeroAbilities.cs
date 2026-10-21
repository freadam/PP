using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// LMB — three-hit melee combo (Riven/Ravenswatch style). The third swing is a wider finisher with knockback.
    /// Consumes State.Buff.Empowered (granted by Tumble) for bonus magic-scaled damage, like Vayne's Tumble.
    /// </summary>
    public class GA_MeleeCombo : DemoGameplayAbility
    {
        public GameplayEffect DamageEffect;
        public float Range = 2.6f;
        public float Angle = 120f;
        public float WindUp = 0.08f;
        public float Recovery = 0.26f;
        public ScalableFloat BaseDamage = new ScalableFloat(12f, 4f);
        public float ADRatio = 1f;
        public float FinisherMultiplier = 1.8f;
        public float ComboWindow = 0.9f;
        public float EmpoweredBonusDamage = 35f;
        public float EmpoweredAPRatio = 0.6f;
        public Color SwingColor = new Color(1f, 0.9f, 0.7f, 0.45f);

        int comboIndex;
        float lastSwingTime = -10f;

        protected override void ActivateAbility(GameplayEventData triggerEventData)
        {
            if (!CommitAbility()) { EndAbility(true); return; }

            var avatar = Avatar;
            avatar.FaceTowards(avatar.AimPoint);

            if (Time.time - lastSwingTime > ComboWindow) comboIndex = 0;
            lastSwingTime = Time.time;
            bool finisher = comboIndex == 2;
            comboIndex = (comboIndex + 1) % 3;

            var wait = AbilityTask_WaitDelay.WaitDelay(this, finisher ? WindUp * 2f : WindUp);
            wait.OnFinish += () => Swing(finisher);
            wait.ReadyForActivation();
        }

        void Swing(bool finisher)
        {
            var avatar = Avatar;
            var origin = avatar.transform.position;
            var forward = avatar.transform.forward;
            float range = finisher ? Range * 1.2f : Range;
            float angle = finisher ? Angle + 60f : Angle;

            var swing = Fx.GroundMesh("Swing", Fx.SectorMesh(range, angle), origin, avatar.transform.rotation,
                finisher ? new Color(1f, 0.7f, 0.3f, 0.55f) : SwingColor);
            var fade = swing.AddComponent<FadeAndDestroy>();
            fade.Lifetime = 0.15f;
            fade.StartScale = Vector3.one * 0.7f;
            fade.EndScale = Vector3.one;
            fade.BaseColor = finisher ? new Color(1f, 0.7f, 0.3f, 0.55f) : SwingColor;

            float mult = finisher ? FinisherMultiplier : 1f;
            float damage = BaseDamage.GetValueAtLevel(GetAbilityLevel()) * mult;
            float ap = 0f;
            bool empowered = AbilitySystem.HasMatchingGameplayTag(GameTags.State_Empowered);
            if (empowered)
            {
                damage += EmpoweredBonusDamage;
                ap = EmpoweredAPRatio;
                AbilitySystem.RemoveActiveEffectsWithTags(GameplayTagContainer.Of(GameTags.State_Empowered));
                Fx.Burst(origin + forward * range * 0.5f + Vector3.up, new Color(0.75f, 0.35f, 1f, 0.6f), 0.5f, range * 1.4f, 0.2f);
            }

            var spec = MakeDamageSpec(DamageEffect, damage, ADRatio * mult, ap);
            foreach (var target in Targeting.HostilesInCone(OwnerTeam, origin, forward, range, angle))
            {
                ApplyToTarget(spec, target, GameTags.Event_Hit_Basic);
                if (finisher || empowered) target.ApplyKnockback(target.transform.position - origin, finisher ? 10f : 7f);
            }

            var recover = AbilityTask_WaitDelay.WaitDelay(this, finisher ? Recovery * 1.5f : Recovery);
            recover.OnFinish += () => EndAbility();
            recover.ReadyForActivation();
        }
    }

    /// <summary>
    /// Skillshot projectile (Ezreal's Mystic Shot for the hero; also the enemy caster's bolt with a wind-up).
    /// The ability builds the effect specs and the projectile delivers them.
    /// </summary>
    public class GA_FireProjectile : DemoGameplayAbility
    {
        public GameplayEffect DamageEffect;
        public GameplayEffect[] ExtraEffects = new GameplayEffect[0];
        public ScalableFloat BaseDamage = new ScalableFloat(40f, 25f);
        public float ADRatio = 1.1f;
        public float APRatio = 0.4f;
        public float Speed = 24f;
        public float Range = 13f;
        public float Radius = 0.45f;
        public float VisualSize = 0.45f;
        public bool Pierce;
        public Color Color = new Color(0.3f, 0.85f, 1f);
        public GameplayTag HitEventTag;
        [Tooltip("0 = fire instantly. Otherwise shows a lane telegraph and locks the caster.")]
        public float WindUp;

        GameObject telegraph;

        protected override void ActivateAbility(GameplayEventData triggerEventData)
        {
            if (!CommitAbility()) { EndAbility(true); return; }

            var avatar = Avatar;
            var direction = avatar.AimDirection;
            avatar.FaceTowards(avatar.transform.position + direction);

            if (WindUp <= 0f)
            {
                Fire(direction);
                EndAbility();
                return;
            }

            telegraph = Fx.GroundMesh("ProjectileTelegraph", Fx.LaneMesh(Range, Radius * 2f), avatar.transform.position,
                Quaternion.LookRotation(direction, Vector3.up), Color.WithAlpha(0.25f));
            var wait = AbilityTask_WaitDelay.WaitDelay(this, WindUp);
            wait.OnFinish += () =>
            {
                Fire(direction);
                EndAbility();
            };
            wait.ReadyForActivation();
        }

        void Fire(Vector3 direction)
        {
            var avatar = Avatar;
            var origin = avatar.transform.position + Vector3.up + direction * (avatar.Radius + 0.3f);
            var projectile = Projectile.Spawn(origin, direction, Color, VisualSize);
            projectile.Instigator = AbilitySystem;
            projectile.Team = OwnerTeam;
            projectile.Speed = Speed;
            projectile.Range = Range;
            projectile.Radius = Radius;
            projectile.Pierce = Pierce;
            projectile.HitEventTag = HitEventTag;
            projectile.EffectSpecs.Add(MakeDamageSpec(DamageEffect, BaseDamage.GetValueAtLevel(GetAbilityLevel()), ADRatio, APRatio));
            foreach (var extra in ExtraEffects)
                if (extra != null) projectile.EffectSpecs.Add(MakeOutgoingGameplayEffectSpec(extra));
            Fx.Burst(origin, Color.WithAlpha(0.6f), 0.3f, 1.2f, 0.15f);
        }

        protected override void OnEndAbility(bool wasCancelled)
        {
            if (telegraph != null) Destroy(telegraph);
        }
    }

    /// <summary>Q — ground-targeted zone that burns and slows enemies standing in it (Frost Field).</summary>
    public class GA_GroundZone : DemoGameplayAbility
    {
        public GameplayEffect DamageEffect;
        public GameplayEffect SlowEffect;
        public float MaxCastRange = 9f;
        public float Radius = 3.2f;
        public ScalableFloat ZoneDuration = new ScalableFloat(4f, 0.5f);
        public float TickInterval = 0.5f;
        public ScalableFloat DamagePerTick = new ScalableFloat(10f, 5f);
        public float APRatioPerTick = 0.12f;
        public Color Color = new Color(0.45f, 0.8f, 1f, 0.35f);

        protected override void ActivateAbility(GameplayEventData triggerEventData)
        {
            if (!CommitAbility()) { EndAbility(true); return; }

            var avatar = Avatar;
            var origin = avatar.transform.position;
            var offset = Vector3.ClampMagnitude(Targeting.Flat(avatar.AimPoint - origin), MaxCastRange);
            var center = origin + offset;
            avatar.FaceTowards(center);

            int level = GetAbilityLevel();
            var zone = GroundZone.Spawn(center, Radius, Color);
            zone.Instigator = AbilitySystem;
            zone.Team = OwnerTeam;
            zone.Duration = ZoneDuration.GetValueAtLevel(level);
            zone.TickInterval = TickInterval;
            zone.EffectSpecs.Add(MakeDamageSpec(DamageEffect, DamagePerTick.GetValueAtLevel(level), 0f, APRatioPerTick));
            if (SlowEffect != null) zone.EffectSpecs.Add(MakeOutgoingGameplayEffectSpec(SlowEffect));

            Fx.GroundRing(center, Color.WithAlpha(0.8f), Radius, 0.3f);
            EndAbility();
        }
    }

    /// <summary>
    /// Space — dash toward movement input (or the cursor). Invulnerable while dashing via ActivationOwnedTags,
    /// cancels the basic attack, and empowers the next one (Vayne's Tumble).
    /// </summary>
    public class GA_Dash : DemoGameplayAbility
    {
        public GameplayEffect EmpowerEffect;
        public float Distance = 5.5f;
        public float Duration = 0.17f;
        public Color TrailColor = new Color(0.75f, 0.45f, 1f, 0.35f);

        protected override void ActivateAbility(GameplayEventData triggerEventData)
        {
            if (!CommitAbility()) { EndAbility(true); return; }

            var avatar = Avatar;
            var direction = avatar.MoveInput.sqrMagnitude > 0.01f ? avatar.MoveInput : avatar.AimDirection;
            var dash = AbilityTask_Dash.Dash(this, avatar, direction, Distance, Duration, TrailColor);
            dash.OnFinish += () =>
            {
                if (EmpowerEffect != null) ApplyGameplayEffectToOwner(EmpowerEffect);
                EndAbility();
            };
            dash.ReadyForActivation();
        }
    }

    /// <summary>
    /// R — channel briefly, then slam: damage, knockback and a stun whose duration scales with ability level
    /// (a SetDuration override on the stun spec).
    /// </summary>
    public class GA_Shockwave : DemoGameplayAbility
    {
        public GameplayEffect DamageEffect;
        public GameplayEffect StunEffect;
        public float Radius = 5.5f;
        public float WindUp = 0.55f;
        public float Recovery = 0.2f;
        public ScalableFloat BaseDamage = new ScalableFloat(110f, 60f);
        public float ADRatio = 0.6f;
        public float APRatio = 0.9f;
        public ScalableFloat StunDuration = new ScalableFloat(1.25f, 0.25f);
        public float Knockback = 12f;

        GameObject telegraph;

        protected override void ActivateAbility(GameplayEventData triggerEventData)
        {
            if (!CommitAbility()) { EndAbility(true); return; }

            var avatar = Avatar;
            telegraph = Telegraphs.Create(avatar.transform.position, Quaternion.identity, Radius, 360f, WindUp, new Color(1f, 0.8f, 0.2f, 0.3f));

            var wait = AbilityTask_WaitDelay.WaitDelay(this, WindUp);
            wait.OnFinish += Slam;
            wait.ReadyForActivation();
        }

        void Slam()
        {
            var avatar = Avatar;
            var origin = avatar.transform.position;
            int level = GetAbilityLevel();

            AbilitySystem.ExecuteGameplayCue(GameTags.Cue_Shockwave, new GameplayCueParameters { Location = origin, HasLocation = true, Instigator = AbilitySystem });
            TopDownCamera.Shake(0.35f, 0.4f);

            var damage = MakeDamageSpec(DamageEffect, BaseDamage.GetValueAtLevel(level), ADRatio, APRatio);
            var stun = MakeOutgoingGameplayEffectSpec(StunEffect);
            stun.SetDuration(StunDuration.GetValueAtLevel(level));

            foreach (var target in Targeting.HostilesInRadius(OwnerTeam, origin, Radius))
            {
                ApplyToTarget(damage, target, GameTags.Event_Hit_Ability);
                ApplyToTarget(stun, target);
                target.ApplyKnockback(target.transform.position - origin, Knockback);
            }

            if (telegraph != null) Destroy(telegraph);
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
