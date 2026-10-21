using System;
using GAS;
using UnityEngine;

namespace GASDemo
{
    [Serializable]
    public struct CharacterStats
    {
        public float MaxHealth;
        public float MaxMana;
        public float AttackPower;
        public float AbilityPower;
        public float Armor;
        public float MoveSpeed;
    }

    /// <summary>
    /// The avatar: a body that moves and reacts. It knows nothing about specific abilities — it only reads
    /// attributes (MoveSpeed) and tags (Stun, Casting, Dead) from its ASC, exactly like an ACharacter with GAS.
    /// Controllers (HeroInput, EnemyBrain) write MoveInput/AimPoint and press ability inputs on the ASC.
    /// </summary>
    [RequireComponent(typeof(AbilitySystemComponent))]
    public class CharacterBase : MonoBehaviour, IAbilitySystemInterface
    {
        public static float ArenaRadius = 22f;

        public Team Team;
        public string DisplayName = "Character";
        public float Radius = 0.5f;
        public float HealthBarHeight = 2.3f;
        public bool DestroyOnDeath = true;
        public Color BodyColor = Color.gray;

        /// <summary>World-space point abilities aim at (mouse for the hero, target for AI).</summary>
        [NonSerialized] public Vector3 AimPoint;
        /// <summary>Desired movement direction on the ground plane, magnitude 0..1.</summary>
        [NonSerialized] public Vector3 MoveInput;

        public AbilitySystemComponent AbilitySystem { get; private set; }
        public CharacterAttributeSet Attributes { get; private set; }
        public bool IsAlive => !isDead;
        public event Action<CharacterBase> OnDied;

        Renderer[] bodyRenderers = Array.Empty<Renderer>();
        Transform body;
        Vector3 externalVelocity;
        float flashTimer;
        bool isDead;

        public AbilitySystemComponent GetAbilitySystemComponent() => AbilitySystem;

        protected virtual void Awake()
        {
            AbilitySystem = GetComponent<AbilitySystemComponent>();
            Attributes = AbilitySystem.AddAttributeSet(new CharacterAttributeSet());
            AimPoint = transform.position + transform.forward;

            AbilitySystem.RegisterGameplayTagEvent(GameTags.State_Dead, OnDeadTagChanged);
            AbilitySystem.RegisterGameplayTagEvent(GameTags.State_Debuff_Stun, OnStunTagChanged);
            AbilitySystem.OnGameplayEvent += HandleGameplayEvent;
        }

        protected virtual void OnEnable() => Targeting.Register(this);
        protected virtual void OnDisable() => Targeting.Unregister(this);

        public void InitializeStats(CharacterStats stats)
        {
            var asc = AbilitySystem;
            asc.SetNumericAttributeBase(CharacterAttributeSet.MaxHealth, stats.MaxHealth);
            asc.SetNumericAttributeBase(CharacterAttributeSet.MaxMana, Mathf.Max(1f, stats.MaxMana));
            asc.SetNumericAttributeBase(CharacterAttributeSet.AttackPower, stats.AttackPower);
            asc.SetNumericAttributeBase(CharacterAttributeSet.AbilityPower, stats.AbilityPower);
            asc.SetNumericAttributeBase(CharacterAttributeSet.Armor, stats.Armor);
            asc.SetNumericAttributeBase(CharacterAttributeSet.MoveSpeed, stats.MoveSpeed);
            RefillVitals();
        }

        public void RefillVitals()
        {
            AbilitySystem.SetNumericAttributeBase(CharacterAttributeSet.Health, AbilitySystem.GetNumericAttribute(CharacterAttributeSet.MaxHealth));
            AbilitySystem.SetNumericAttributeBase(CharacterAttributeSet.Mana, AbilitySystem.GetNumericAttribute(CharacterAttributeSet.MaxMana));
        }

        public void SetBody(Transform bodyRoot)
        {
            body = bodyRoot;
            bodyRenderers = bodyRoot.GetComponentsInChildren<Renderer>();
            Fx.SetColor(bodyRenderers, BodyColor);
        }

        public bool HasTag(GameplayTag tag) => AbilitySystem.HasMatchingGameplayTag(tag);

        public bool CanMove =>
            !isDead && !HasTag(GameTags.State_Debuff_Stun) && !HasTag(GameTags.State_Casting) && !HasTag(GameTags.State_Dashing);

        protected virtual void Update()
        {
            if (isDead) return;
            float dt = Time.deltaTime;
            if (dt <= 0f) return;

            var velocity = Vector3.zero;
            if (CanMove && MoveInput.sqrMagnitude > 0.0001f)
            {
                float speed = AbilitySystem.GetNumericAttribute(CharacterAttributeSet.MoveSpeed);
                if (HasTag(GameTags.State_Attacking)) speed *= 0.45f;
                var dir = Vector3.ClampMagnitude(Targeting.Flat(MoveInput), 1f);
                velocity = dir * speed;
                if (!HasTag(GameTags.State_Attacking))
                    FaceDirection(dir, 18f * dt);
            }

            velocity += externalVelocity;
            externalVelocity = Vector3.MoveTowards(externalVelocity, Vector3.zero, 40f * dt);
            Move(velocity * dt);
            ResolveOverlaps();
            UpdateFlash(dt);
        }

        /// <summary>Moves on the ground plane, kept inside the arena.</summary>
        public void Move(Vector3 delta)
        {
            var p = transform.position + Targeting.Flat(delta);
            var flat = Targeting.Flat(p);
            float max = ArenaRadius - Radius;
            if (flat.magnitude > max) flat = flat.normalized * max;
            transform.position = new Vector3(flat.x, 0f, flat.z);
        }

        public void FaceTowards(Vector3 worldPoint)
        {
            var dir = Targeting.Flat(worldPoint - transform.position);
            if (dir.sqrMagnitude > 0.0001f) transform.rotation = Quaternion.LookRotation(dir.normalized, Vector3.up);
        }

        void FaceDirection(Vector3 dir, float t)
        {
            if (dir.sqrMagnitude < 0.0001f) return;
            transform.rotation = Quaternion.Slerp(transform.rotation, Quaternion.LookRotation(dir.normalized, Vector3.up), Mathf.Clamp01(t));
        }

        public Vector3 AimDirection
        {
            get
            {
                var d = Targeting.Flat(AimPoint - transform.position);
                return d.sqrMagnitude > 0.0001f ? d.normalized : transform.forward;
            }
        }

        public void ApplyKnockback(Vector3 direction, float strength)
        {
            if (isDead) return;
            externalVelocity = Targeting.Flat(direction).normalized * strength;
        }

        void ResolveOverlaps()
        {
            foreach (var other in Targeting.All)
            {
                if (other == this || !other.IsAlive) continue;
                var delta = Targeting.Flat(transform.position - other.transform.position);
                float minDist = Radius + other.Radius;
                float dist = delta.magnitude;
                if (dist >= minDist || dist < 0.0001f) continue;
                Move(delta / dist * (minDist - dist) * 0.5f);
            }
        }

        // ------------------------------------------------------------------------------------- reactions

        void HandleGameplayEvent(GameplayTag tag, GameplayEventData data)
        {
            if (!tag.MatchesTag(GameTags.Event_Damaged)) return;
            flashTimer = 0.12f;
            var color = Team == Team.Heroes ? new Color(1f, 0.35f, 0.3f) : Color.white;
            FloatingText.Spawn(transform.position + Vector3.up * HealthBarHeight, Mathf.RoundToInt(data.EventMagnitude).ToString(), color);
        }

        void UpdateFlash(float dt)
        {
            if (flashTimer <= 0f) return;
            flashTimer -= dt;
            Fx.SetColor(bodyRenderers, flashTimer > 0f ? Color.white : BodyColor);
        }

        void OnStunTagChanged(GameplayTag tag, int newCount)
        {
            // Crowd control interrupts everything except passives.
            if (newCount > 0)
                AbilitySystem.CancelAbilities(GameplayTagContainer.Of(GameTags.Ability), GameplayTagContainer.Of(GameTags.Ability_Passive));
        }

        void OnDeadTagChanged(GameplayTag tag, int newCount)
        {
            if (newCount <= 0 || isDead) return;
            isDead = true;
            AbilitySystem.CancelAllAbilities();
            Targeting.Unregister(this);
            Fx.SetColor(bodyRenderers, Color.Lerp(BodyColor, Color.black, 0.6f));
            if (body != null) body.localRotation = Quaternion.Euler(-80f, 0f, 0f);
            OnDied?.Invoke(this);
            if (DestroyOnDeath) Destroy(gameObject, 1.2f);
        }
    }
}
