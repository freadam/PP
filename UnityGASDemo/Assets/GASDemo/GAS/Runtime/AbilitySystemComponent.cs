using System;
using System.Collections.Generic;
using UnityEngine;

namespace GAS
{
    /// <summary>
    /// UAbilitySystemComponent: the hub every gameplay actor carries. It owns
    ///  - attribute sets (numbers),
    ///  - owned gameplay tags (state, ref-counted),
    ///  - active gameplay effects (timed changes to numbers and tags),
    ///  - granted abilities (actions, bound to input / triggered by events and tags),
    /// and is the only door through which any of them change. Other systems talk to an actor through its ASC:
    /// apply a spec, send an event, try to activate by tag — never by reaching into its components.
    /// </summary>
    [DisallowMultipleComponent]
    public class AbilitySystemComponent : MonoBehaviour
    {
        readonly List<AttributeSet> attributeSets = new List<AttributeSet>();
        readonly Dictionary<GameplayAttribute, Action<float, float>> attributeChangeDelegates = new Dictionary<GameplayAttribute, Action<float, float>>();
        readonly GameplayTagCountContainer blockedAbilityTags = new GameplayTagCountContainer();
        readonly List<GameplayAbilitySpec> abilities = new List<GameplayAbilitySpec>();
        readonly Dictionary<GameplayAbilitySpecHandle, List<(GameplayTag tag, Action<GameplayTag, int> handler)>> tagTriggerBindings =
            new Dictionary<GameplayAbilitySpecHandle, List<(GameplayTag, Action<GameplayTag, int>)>>();

        ActiveGameplayEffectsContainer activeEffects;
        int nextAbilityHandle = 1;

        public GameplayTagCountContainer OwnedTags { get; } = new GameplayTagCountContainer();
        public ActiveGameplayEffectsContainer ActiveEffects => activeEffects ??= new ActiveGameplayEffectsContainer(this);
        public IReadOnlyList<GameplayAbilitySpec> ActivatableAbilities => abilities;
        public IReadOnlyList<AttributeSet> AttributeSets => attributeSets;

        /// <summary>The logical owner (a PlayerState in UE). Here the avatar owns its own ASC.</summary>
        public GameObject OwnerActor => gameObject;
        /// <summary>The physical body abilities act through.</summary>
        public GameObject AvatarActor => gameObject;

        public event Action<GameplayAbility> OnAbilityActivated;
        public event Action<GameplayAbility, bool> OnAbilityEnded;
        public event Action<GameplayAbility, GameplayTagContainer> OnAbilityFailed;
        public event Action<GameplayTag, GameplayEventData> OnGameplayEvent;
        public event Action<GameplayAttribute, float, float> OnAnyAttributeChanged;
        public event Action<GameplayEffectSpec, ActiveGameplayEffectHandle> OnGameplayEffectAppliedToSelf;
        public event Action<AbilitySystemComponent, GameplayEffectSpec, ActiveGameplayEffectHandle> OnGameplayEffectAppliedToTarget;

        void Update()
        {
            float dt = Time.deltaTime;
            if (dt <= 0f) return;

            ActiveEffects.Tick(dt);

            // Snapshot: abilities can grant/clear abilities or send nested events while we iterate.
            foreach (var spec in abilities.ToArray())
                if (spec.IsActive) spec.Ability.TickTasks(dt);
        }

        void OnDestroy()
        {
            CancelAllAbilities();
            foreach (var spec in abilities)
                if (spec.Ability != null) Destroy(spec.Ability);
            abilities.Clear();
        }

        // =================================================================== attributes

        public T AddAttributeSet<T>(T set) where T : AttributeSet
        {
            set.Owner = this;
            attributeSets.Add(set);
            return set;
        }

        public T GetAttributeSet<T>() where T : AttributeSet
        {
            foreach (var s in attributeSets)
                if (s is T typed) return typed;
            return null;
        }

        bool TryFindAttribute(GameplayAttribute attribute, out AttributeSet set, out GameplayAttributeData data)
        {
            foreach (var s in attributeSets)
            {
                if (s.TryGetData(attribute, out data))
                {
                    set = s;
                    return true;
                }
            }
            set = null;
            data = null;
            return false;
        }

        public bool HasAttribute(GameplayAttribute attribute) => TryFindAttribute(attribute, out _, out _);

        public float GetNumericAttribute(GameplayAttribute attribute) =>
            TryFindAttribute(attribute, out _, out var data) ? data.CurrentValue : 0f;

        public float GetNumericAttributeBase(GameplayAttribute attribute) =>
            TryFindAttribute(attribute, out _, out var data) ? data.BaseValue : 0f;

        public void SetNumericAttributeBase(GameplayAttribute attribute, float value)
        {
            if (!TryFindAttribute(attribute, out var set, out var data)) return;
            set.PreAttributeBaseChange(attribute, ref value);
            data.BaseValue = value;
            RecalculateCurrentValue(attribute);
        }

        /// <summary>Equivalent of GetGameplayAttributeValueChangeDelegate: (oldValue, newValue).</summary>
        public void RegisterAttributeChanged(GameplayAttribute attribute, Action<float, float> handler)
        {
            attributeChangeDelegates.TryGetValue(attribute, out var existing);
            attributeChangeDelegates[attribute] = existing + handler;
        }

        public void UnregisterAttributeChanged(GameplayAttribute attribute, Action<float, float> handler)
        {
            if (attributeChangeDelegates.TryGetValue(attribute, out var existing))
                attributeChangeDelegates[attribute] = existing - handler;
        }

        internal void RecalculateCurrentValue(GameplayAttribute attribute)
        {
            if (!TryFindAttribute(attribute, out var set, out var data)) return;
            float newValue = ActiveEffects.Aggregate(attribute, data.BaseValue);
            set.PreAttributeChange(attribute, ref newValue);
            float oldValue = data.CurrentValue;
            data.CurrentValue = newValue;
            if (oldValue == newValue) return;

            set.PostAttributeChange(attribute, oldValue, newValue);
            if (attributeChangeDelegates.TryGetValue(attribute, out var handler)) handler?.Invoke(oldValue, newValue);
            OnAnyAttributeChanged?.Invoke(attribute, oldValue, newValue);
        }

        /// <summary>Instant/periodic modification of a base value, with the attribute set's Pre/PostGameplayEffectExecute hooks.</summary>
        internal void ApplyModToAttributeBase(GameplayEffectSpec spec, GameplayModifierEvaluatedData evaluated)
        {
            if (!TryFindAttribute(evaluated.Attribute, out var set, out var data)) return;

            var callback = new GameplayEffectModCallbackData { EffectSpec = spec, EvaluatedData = evaluated, Target = this };
            if (!set.PreGameplayEffectExecute(callback)) return;

            float oldBase = data.BaseValue;
            float newBase = evaluated.Op switch
            {
                ModifierOp.Add => oldBase + evaluated.Magnitude,
                ModifierOp.Multiply => oldBase * evaluated.Magnitude,
                _ => evaluated.Magnitude,
            };
            SetNumericAttributeBase(evaluated.Attribute, newBase);
            set.PostGameplayEffectExecute(callback);
        }

        // =================================================================== tags

        public bool HasMatchingGameplayTag(GameplayTag tag) => OwnedTags.HasMatchingTag(tag);
        public bool HasAnyMatchingGameplayTags(GameplayTagContainer tags) => OwnedTags.HasAnyMatchingTags(tags);
        public bool HasAllMatchingGameplayTags(GameplayTagContainer tags) => OwnedTags.HasAllMatchingTags(tags);

        /// <summary>Tags not backed by an effect — the caller is responsible for removing them.</summary>
        public void AddLooseGameplayTag(GameplayTag tag, int count = 1) => OwnedTags.UpdateTagCount(tag, count);
        public void RemoveLooseGameplayTag(GameplayTag tag, int count = 1) => OwnedTags.UpdateTagCount(tag, -count);

        public void RegisterGameplayTagEvent(GameplayTag tag, Action<GameplayTag, int> handler) => OwnedTags.RegisterGameplayTagEvent(tag, handler);
        public void UnregisterGameplayTagEvent(GameplayTag tag, Action<GameplayTag, int> handler) => OwnedTags.UnregisterGameplayTagEvent(tag, handler);

        // =================================================================== effects

        public GameplayEffectContext MakeEffectContext() => new GameplayEffectContext { Instigator = this, EffectCauser = AvatarActor };

        public GameplayEffectSpec MakeOutgoingSpec(GameplayEffect effect, float level = 1f, GameplayEffectContext context = null) =>
            new GameplayEffectSpec(effect, context ?? MakeEffectContext(), level);

        public ActiveGameplayEffectHandle ApplyGameplayEffectSpecToSelf(GameplayEffectSpec spec)
        {
            if (spec == null || spec.Def == null) return ActiveGameplayEffectHandle.Invalid;
            var handle = ActiveEffects.Apply(spec);
            if (handle.WasSuccessfullyApplied) OnGameplayEffectAppliedToSelf?.Invoke(spec, handle);
            return handle;
        }

        public ActiveGameplayEffectHandle ApplyGameplayEffectSpecToTarget(GameplayEffectSpec spec, AbilitySystemComponent target)
        {
            if (target == null) return ActiveGameplayEffectHandle.Invalid;
            var handle = target.ApplyGameplayEffectSpecToSelf(spec);
            if (handle.WasSuccessfullyApplied) OnGameplayEffectAppliedToTarget?.Invoke(target, spec, handle);
            return handle;
        }

        public ActiveGameplayEffectHandle ApplyGameplayEffectToSelf(GameplayEffect effect, float level = 1f) =>
            ApplyGameplayEffectSpecToSelf(MakeOutgoingSpec(effect, level));

        public bool RemoveActiveGameplayEffect(ActiveGameplayEffectHandle handle, int stacksToRemove = -1) =>
            ActiveEffects.Remove(ActiveEffects.GetActiveEffect(handle), stacksToRemove);

        /// <summary>Removes active effects whose asset or granted tags match any of <paramref name="tags"/>.</summary>
        public int RemoveActiveEffectsWithTags(GameplayTagContainer tags) => ActiveEffects.RemoveActiveEffectsWithTags(tags);

        // =================================================================== cues

        internal void InvokeGameplayCues(GameplayEffectSpec spec, GameplayCueEvent cueEvent)
        {
            var cues = spec.Def.GameplayCues;
            if (cues.IsEmpty) return;
            var parameters = new GameplayCueParameters
            {
                Instigator = spec.Context.Instigator,
                EffectCauser = spec.Context.EffectCauser,
                Location = spec.Context.HasHitLocation ? spec.Context.HitLocation : transform.position,
                HasLocation = true,
                SourceObject = spec.Context.SourceObject,
            };
            foreach (var cue in cues)
                GameplayCueManager.HandleGameplayCue(AvatarActor, cue, cueEvent, parameters);
        }

        public void ExecuteGameplayCue(GameplayTag cueTag, GameplayCueParameters parameters) =>
            GameplayCueManager.HandleGameplayCue(AvatarActor, cueTag, GameplayCueEvent.Executed, parameters);

        public void AddGameplayCue(GameplayTag cueTag, GameplayCueParameters parameters)
        {
            GameplayCueManager.HandleGameplayCue(AvatarActor, cueTag, GameplayCueEvent.OnActive, parameters);
            GameplayCueManager.HandleGameplayCue(AvatarActor, cueTag, GameplayCueEvent.WhileActive, parameters);
        }

        public void RemoveGameplayCue(GameplayTag cueTag) =>
            GameplayCueManager.HandleGameplayCue(AvatarActor, cueTag, GameplayCueEvent.Removed, default);

        // =================================================================== abilities

        public GameplayAbilitySpecHandle GiveAbility(GameplayAbility ability, int level = 1, int inputId = -1, object sourceObject = null)
        {
            if (ability == null) return default;

            var spec = new GameplayAbilitySpec
            {
                Handle = new GameplayAbilitySpecHandle(nextAbilityHandle++),
                Definition = ability,
                Level = level,
                InputID = inputId,
                SourceObject = sourceObject,
            };
            var instance = Instantiate(ability);
            instance.name = ability.name;
            spec.Ability = instance;
            abilities.Add(spec);
            instance.OnGiveAbility(this, spec);

            BindTagTriggers(spec);

            if (instance.ActivateAbilityOnGranted) TryActivateAbility(spec.Handle);
            return spec.Handle;
        }

        public void ClearAbility(GameplayAbilitySpecHandle handle)
        {
            var spec = FindAbilitySpecFromHandle(handle);
            if (spec == null) return;
            UnbindTagTriggers(spec);
            spec.Ability.OnRemoveAbility();
            abilities.Remove(spec);
            Destroy(spec.Ability);
        }

        public GameplayAbilitySpec FindAbilitySpecFromHandle(GameplayAbilitySpecHandle handle)
        {
            foreach (var s in abilities)
                if (s.Handle.Equals(handle)) return s;
            return null;
        }

        public GameplayAbilitySpec FindAbilitySpecFromDefinition(GameplayAbility definition)
        {
            foreach (var s in abilities)
                if (s.Definition == definition) return s;
            return null;
        }

        public GameplayAbilitySpec FindAbilitySpecFromInputID(int inputId)
        {
            foreach (var s in abilities)
                if (s.InputID == inputId) return s;
            return null;
        }

        public bool TryActivateAbility(GameplayAbilitySpecHandle handle, GameplayEventData triggerEventData = null)
        {
            var spec = FindAbilitySpecFromHandle(handle);
            if (spec == null) return false;

            var ability = spec.Ability;
            var failureTags = new GameplayTagContainer();
            if (!ability.CanActivateAbility(triggerEventData, failureTags))
            {
                OnAbilityFailed?.Invoke(ability, failureTags);
                return false;
            }
            ability.CallActivateAbility(triggerEventData);
            return true;
        }

        /// <summary>Activates every granted, inactive ability whose AbilityTags match. AI uses this: "do an Ability.Attack".</summary>
        public bool TryActivateAbilitiesByTag(GameplayTagContainer tags)
        {
            bool any = false;
            foreach (var spec in abilities.ToArray())
                if (!spec.IsActive && spec.Ability.AbilityTags.HasAny(tags))
                    any |= TryActivateAbility(spec.Handle);
            return any;
        }

        public void AbilityLocalInputPressed(int inputId)
        {
            foreach (var spec in abilities.ToArray())
            {
                if (spec.InputID != inputId) continue;
                spec.InputPressed = true;
                if (spec.IsActive) spec.Ability.InputPressed();
                else TryActivateAbility(spec.Handle);
            }
        }

        public void AbilityLocalInputReleased(int inputId)
        {
            foreach (var spec in abilities.ToArray())
            {
                if (spec.InputID != inputId) continue;
                spec.InputPressed = false;
                if (spec.IsActive) spec.Ability.InputReleased();
            }
        }

        /// <summary>Cancel active abilities with any of <paramref name="withTags"/> (null = all), except those with <paramref name="withoutTags"/>.</summary>
        public void CancelAbilities(GameplayTagContainer withTags = null, GameplayTagContainer withoutTags = null, GameplayAbility ignore = null)
        {
            foreach (var spec in abilities.ToArray())
            {
                var ability = spec.Ability;
                if (!spec.IsActive || ability == ignore) continue;
                bool matches = withTags == null || ability.AbilityTags.HasAny(withTags);
                bool excluded = withoutTags != null && ability.AbilityTags.HasAny(withoutTags);
                if (matches && !excluded) ability.CancelAbility();
            }
        }

        public void CancelAllAbilities(GameplayAbility ignore = null) => CancelAbilities(null, null, ignore);

        /// <summary>True if any of the ability's tags (or their parents) is currently blocked.</summary>
        public bool AreAbilityTagsBlocked(GameplayTagContainer abilityTags)
        {
            foreach (var tag in abilityTags)
                foreach (var t in tag.GetSelfAndParents())
                    if (blockedAbilityTags.HasTagExact(t)) return true;
            return false;
        }

        internal void ApplyAbilityBlockAndCancelTags(GameplayAbility ability, GameplayTagContainer block, GameplayTagContainer cancel, bool enable)
        {
            blockedAbilityTags.UpdateTagCount(block, enable ? 1 : -1);
            if (enable && !cancel.IsEmpty) CancelAbilities(cancel, null, ability);
        }

        internal void NotifyAbilityActivated(GameplayAbility ability) => OnAbilityActivated?.Invoke(ability);
        internal void NotifyAbilityEnded(GameplayAbility ability, bool wasCancelled) => OnAbilityEnded?.Invoke(ability, wasCancelled);

        // =================================================================== events & triggers

        /// <summary>Send a gameplay event to this ASC: activates abilities triggered by it, then notifies listeners. Returns how many abilities activated.</summary>
        public int HandleGameplayEvent(GameplayTag eventTag, GameplayEventData payload)
        {
            payload ??= new GameplayEventData();
            payload.EventTag = eventTag;

            int activated = 0;
            foreach (var spec in abilities.ToArray())
            {
                foreach (var trigger in spec.Ability.AbilityTriggers)
                {
                    if (trigger.TriggerSource != AbilityTriggerSource.GameplayEvent || !eventTag.MatchesTag(trigger.TriggerTag)) continue;
                    if (TryActivateAbility(spec.Handle, payload)) activated++;
                    break;
                }
            }

            OnGameplayEvent?.Invoke(eventTag, payload);
            return activated;
        }

        void BindTagTriggers(GameplayAbilitySpec spec)
        {
            foreach (var trigger in spec.Ability.AbilityTriggers)
            {
                if (trigger.TriggerSource == AbilityTriggerSource.GameplayEvent) continue;
                var source = trigger.TriggerSource;
                var handle = spec.Handle;
                Action<GameplayTag, int> handler = (tag, newCount) =>
                {
                    var s = FindAbilitySpecFromHandle(handle);
                    if (s == null) return;
                    if (newCount > 0 && !s.IsActive) TryActivateAbility(handle, new GameplayEventData { EventTag = tag });
                    else if (newCount == 0 && source == AbilityTriggerSource.OwnedTagPresent && s.IsActive) s.Ability.CancelAbility();
                };
                OwnedTags.RegisterGameplayTagEvent(trigger.TriggerTag, handler);
                if (!tagTriggerBindings.TryGetValue(handle, out var list)) tagTriggerBindings[handle] = list = new List<(GameplayTag, Action<GameplayTag, int>)>();
                list.Add((trigger.TriggerTag, handler));

                if (OwnedTags.HasMatchingTag(trigger.TriggerTag)) handler(trigger.TriggerTag, OwnedTags.GetTagCount(trigger.TriggerTag));
            }
        }

        void UnbindTagTriggers(GameplayAbilitySpec spec)
        {
            if (!tagTriggerBindings.TryGetValue(spec.Handle, out var list)) return;
            foreach (var (tag, handler) in list) OwnedTags.UnregisterGameplayTagEvent(tag, handler);
            tagTriggerBindings.Remove(spec.Handle);
        }
    }
}
