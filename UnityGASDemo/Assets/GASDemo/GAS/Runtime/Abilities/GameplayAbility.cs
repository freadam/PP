using System.Collections.Generic;
using UnityEngine;

namespace GAS
{
    /// <summary>
    /// UGameplayAbility. A ScriptableObject subclass per ability kind (like a Blueprint GA class); its serialized
    /// fields are the designer-tunable defaults. When granted, the ASC clones it, so each actor gets its own
    /// instance (InstancedPerActor) and subclasses may keep per-activation state in plain fields.
    ///
    /// Lifecycle: CanActivateAbility -> (activation tags/blocking/cancelling) -> ActivateAbility ->
    /// CommitAbility (pay cost, start cooldown) -> AbilityTasks run -> EndAbility.
    /// </summary>
    public abstract class GameplayAbility : ScriptableObject
    {
        [Header("Tags")]
        [Tooltip("Describe this ability (\"Ability.Skill.Fireball\"). Used by cancel/block/activate-by-tag.")]
        public GameplayTagContainer AbilityTags = new GameplayTagContainer();
        [Tooltip("Active abilities with these tags are cancelled when this activates.")]
        public GameplayTagContainer CancelAbilitiesWithTag = new GameplayTagContainer();
        [Tooltip("Abilities with these tags cannot activate while this one is active.")]
        public GameplayTagContainer BlockAbilitiesWithTag = new GameplayTagContainer();
        [Tooltip("Granted to the owner while this ability is active.")]
        public GameplayTagContainer ActivationOwnedTags = new GameplayTagContainer();
        [Tooltip("The owner needs all of these to activate.")]
        public GameplayTagContainer ActivationRequiredTags = new GameplayTagContainer();
        [Tooltip("The owner must have none of these to activate.")]
        public GameplayTagContainer ActivationBlockedTags = new GameplayTagContainer();

        [Header("Cost")]
        [Tooltip("Instant effect applied on commit. CheckCost refuses activation if it would drive an attribute below zero.")]
        public GameplayEffect CostEffect;

        [Header("Cooldown")]
        [Tooltip("HasDuration effect applied on commit. Its granted tags (plus CooldownTags) mark the ability as cooling down.")]
        public GameplayEffect CooldownEffect;
        [Tooltip("Overrides the cooldown effect's duration when > 0, so one generic cooldown effect can serve every ability.")]
        public ScalableFloat CooldownDuration;
        public GameplayTagContainer CooldownTags = new GameplayTagContainer();

        [Header("Triggers")]
        public List<AbilityTriggerData> AbilityTriggers = new List<AbilityTriggerData>();
        [Tooltip("Try to activate as soon as the ability is granted (passives).")]
        public bool ActivateAbilityOnGranted;

        readonly List<AbilityTask> activeTasks = new List<AbilityTask>();

        public AbilitySystemComponent AbilitySystem { get; private set; }
        public GameplayAbilitySpec Spec { get; private set; }
        public bool IsActive { get; private set; }
        public GameplayEventData CurrentEventData { get; private set; }

        public GameObject AvatarActor => AbilitySystem != null ? AbilitySystem.AvatarActor : null;
        public int GetAbilityLevel() => Spec?.Level ?? 1;

        // ---------------------------------------------------------------- lifecycle (called by the ASC)

        internal void OnGiveAbility(AbilitySystemComponent asc, GameplayAbilitySpec spec)
        {
            AbilitySystem = asc;
            Spec = spec;
            OnGranted();
        }

        internal void OnRemoveAbility()
        {
            if (IsActive) EndAbility(true);
            OnRemoved();
        }

        protected virtual void OnGranted() { }
        protected virtual void OnRemoved() { }

        public virtual bool CanActivateAbility(GameplayEventData triggerEventData, GameplayTagContainer failureTags = null)
        {
            if (AbilitySystem == null || IsActive) return false;
            if (!DoesAbilitySatisfyTagRequirements(failureTags)) return false;
            if (!CheckCooldown(failureTags)) return false;
            if (!CheckCost(failureTags)) return false;
            return true;
        }

        protected virtual bool DoesAbilitySatisfyTagRequirements(GameplayTagContainer failureTags)
        {
            var owned = AbilitySystem.OwnedTags;
            if (!owned.HasAllMatchingTags(ActivationRequiredTags)) { failureTags?.AppendTags(ActivationRequiredTags); return false; }
            if (owned.HasAnyMatchingTags(ActivationBlockedTags)) { failureTags?.AppendTags(ActivationBlockedTags); return false; }
            if (AbilitySystem.AreAbilityTagsBlocked(AbilityTags)) { failureTags?.AppendTags(AbilityTags); return false; }
            return true;
        }

        internal void CallActivateAbility(GameplayEventData triggerEventData)
        {
            IsActive = true;
            CurrentEventData = triggerEventData;
            AbilitySystem.OwnedTags.UpdateTagCount(ActivationOwnedTags, +1);
            AbilitySystem.ApplyAbilityBlockAndCancelTags(this, BlockAbilitiesWithTag, CancelAbilitiesWithTag, true);
            AbilitySystem.NotifyAbilityActivated(this);
            ActivateAbility(triggerEventData);
        }

        /// <summary>The ability's body. Call <see cref="CommitAbility"/> when the ability should pay its cost and go on cooldown.</summary>
        protected abstract void ActivateAbility(GameplayEventData triggerEventData);

        /// <summary>Ends the ability (removes activation tags, kills tasks). Safe to call more than once.</summary>
        public void EndAbility(bool wasCancelled = false)
        {
            if (!IsActive) return;
            IsActive = false;

            foreach (var task in activeTasks.ToArray()) task.EndTask();
            activeTasks.Clear();

            OnEndAbility(wasCancelled);

            AbilitySystem.OwnedTags.UpdateTagCount(ActivationOwnedTags, -1);
            AbilitySystem.ApplyAbilityBlockAndCancelTags(this, BlockAbilitiesWithTag, CancelAbilitiesWithTag, false);
            CurrentEventData = null;
            AbilitySystem.NotifyAbilityEnded(this, wasCancelled);
        }

        public void CancelAbility() => EndAbility(true);

        /// <summary>Clean up anything the activation spawned (telegraphs, movement locks).</summary>
        protected virtual void OnEndAbility(bool wasCancelled) { }

        public virtual void InputPressed()
        {
            // Snapshot: a task finishing can end the ability (and every other task) mid-loop.
            foreach (var t in activeTasks.ToArray()) t.OnInputPressed();
        }

        public virtual void InputReleased()
        {
            // Snapshot: a task finishing can end the ability (and every other task) mid-loop.
            foreach (var t in activeTasks.ToArray()) t.OnInputReleased();
        }

        // ---------------------------------------------------------------- cost & cooldown

        /// <summary>Pays the cost and starts the cooldown. Returns false (and changes nothing) if either check fails.</summary>
        public bool CommitAbility()
        {
            if (!CheckCooldown(null) || !CheckCost(null)) return false;
            ApplyCost();
            ApplyCooldown();
            return true;
        }

        public virtual GameplayTagContainer GetCooldownTags()
        {
            var tags = new GameplayTagContainer();
            tags.AppendTags(CooldownTags);
            if (CooldownEffect != null) tags.AppendTags(CooldownEffect.GrantedTags);
            return tags;
        }

        public virtual bool CheckCooldown(GameplayTagContainer failureTags)
        {
            var cooldownTags = GetCooldownTags();
            if (cooldownTags.IsEmpty || !AbilitySystem.OwnedTags.HasAnyMatchingTags(cooldownTags)) return true;
            failureTags?.AppendTags(cooldownTags);
            return false;
        }

        protected virtual float GetCooldownDuration() => CooldownDuration.GetValueAtLevel(GetAbilityLevel());

        protected virtual void ApplyCooldown()
        {
            if (CooldownEffect == null) return;
            var spec = MakeOutgoingGameplayEffectSpec(CooldownEffect);
            float duration = GetCooldownDuration();
            if (duration > 0f) spec.SetDuration(duration);
            spec.DynamicGrantedTags.AppendTags(CooldownTags);
            AbilitySystem.ApplyGameplayEffectSpecToSelf(spec);
        }

        /// <summary>Build the cost spec. Override to fill SetByCaller values (e.g. a per-ability mana cost).</summary>
        protected virtual GameplayEffectSpec MakeCostSpec() => CostEffect != null ? MakeOutgoingGameplayEffectSpec(CostEffect) : null;

        public virtual bool CheckCost(GameplayTagContainer failureTags)
        {
            var spec = MakeCostSpec();
            if (spec == null) return true;
            foreach (var mod in spec.Def.Modifiers)
            {
                if (mod.Op != ModifierOp.Add) continue;
                float magnitude = mod.Magnitude.Calculate(spec, AbilitySystem);
                if (AbilitySystem.GetNumericAttribute(mod.Attribute) + magnitude < 0f)
                {
                    failureTags?.AddTag(GameplayTag.Request("Ability.Fail.Cost"));
                    return false;
                }
            }
            return true;
        }

        protected virtual void ApplyCost()
        {
            var spec = MakeCostSpec();
            if (spec != null) AbilitySystem.ApplyGameplayEffectSpecToSelf(spec);
        }

        /// <summary>Remaining/total cooldown, for UI.</summary>
        public bool GetCooldownTimeRemainingAndDuration(out float timeRemaining, out float duration) =>
            AbilitySystem.ActiveEffects.GetTimeRemainingAndDuration(GetCooldownTags(), out timeRemaining, out duration);

        // ---------------------------------------------------------------- effects

        public GameplayEffectContext MakeEffectContext()
        {
            var ctx = AbilitySystem.MakeEffectContext();
            ctx.Ability = this;
            return ctx;
        }

        public GameplayEffectSpec MakeOutgoingGameplayEffectSpec(GameplayEffect effect, float level = -1f) =>
            new GameplayEffectSpec(effect, MakeEffectContext(), level < 0f ? GetAbilityLevel() : level);

        public ActiveGameplayEffectHandle ApplyGameplayEffectToOwner(GameplayEffect effect, float level = -1f) =>
            AbilitySystem.ApplyGameplayEffectSpecToSelf(MakeOutgoingGameplayEffectSpec(effect, level));

        public ActiveGameplayEffectHandle ApplyGameplayEffectSpecToTarget(GameplayEffectSpec spec, AbilitySystemComponent target) =>
            AbilitySystem.ApplyGameplayEffectSpecToTarget(spec, target);

        // ---------------------------------------------------------------- tasks

        /// <summary>Create a task bound to this activation. Configure it, subscribe to its delegates, then call ReadyForActivation().</summary>
        public T NewAbilityTask<T>() where T : AbilityTask, new()
        {
            var task = new T();
            task.InitTask(this);
            activeTasks.Add(task);
            return task;
        }

        internal void OnTaskEnded(AbilityTask task) => activeTasks.Remove(task);

        internal void TickTasks(float deltaTime)
        {
            if (activeTasks.Count == 0) return;
            // Snapshot: a task finishing can end the ability (and every other task) mid-loop.
            foreach (var t in activeTasks.ToArray())
                if (!t.IsFinished && t.IsTickingTask) t.TickTask(deltaTime);
        }

        public IReadOnlyList<AbilityTask> ActiveTasks => activeTasks;
    }
}
