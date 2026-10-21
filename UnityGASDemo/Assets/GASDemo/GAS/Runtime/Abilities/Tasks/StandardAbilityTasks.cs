using System;

namespace GAS
{
    /// <summary>UAbilityTask_WaitDelay</summary>
    public class AbilityTask_WaitDelay : AbilityTask
    {
        float remaining;
        public event Action OnFinish;
        public override bool IsTickingTask => true;

        public static AbilityTask_WaitDelay WaitDelay(GameplayAbility owningAbility, float seconds)
        {
            var task = owningAbility.NewAbilityTask<AbilityTask_WaitDelay>();
            task.remaining = seconds;
            return task;
        }

        public override void TickTask(float deltaTime)
        {
            remaining -= deltaTime;
            if (remaining > 0f) return;
            bool broadcast = ShouldBroadcastAbilityTaskDelegates;
            EndTask();
            if (broadcast) OnFinish?.Invoke();
        }
    }

    /// <summary>UAbilityTask_WaitInputRelease: reports how long the activating input was held (charge-up abilities).</summary>
    public class AbilityTask_WaitInputRelease : AbilityTask
    {
        float heldTime;
        public event Action<float> OnRelease;
        public override bool IsTickingTask => true;

        public static AbilityTask_WaitInputRelease WaitInputRelease(GameplayAbility owningAbility) =>
            owningAbility.NewAbilityTask<AbilityTask_WaitInputRelease>();

        protected override void Activate()
        {
            // Released before the task started (e.g. a tap): fire straight away.
            if (Ability.Spec != null && !Ability.Spec.InputPressed) OnInputReleased();
        }

        public override void TickTask(float deltaTime) => heldTime += deltaTime;

        public override void OnInputReleased()
        {
            bool broadcast = ShouldBroadcastAbilityTaskDelegates;
            EndTask();
            if (broadcast) OnRelease?.Invoke(heldTime);
        }
    }

    /// <summary>UAbilityTask_WaitGameplayEvent</summary>
    public class AbilityTask_WaitGameplayEvent : AbilityTask
    {
        GameplayTag eventTag;
        bool onlyTriggerOnce;
        public event Action<GameplayEventData> EventReceived;

        public static AbilityTask_WaitGameplayEvent WaitGameplayEvent(GameplayAbility owningAbility, GameplayTag tag, bool onlyTriggerOnce = false)
        {
            var task = owningAbility.NewAbilityTask<AbilityTask_WaitGameplayEvent>();
            task.eventTag = tag;
            task.onlyTriggerOnce = onlyTriggerOnce;
            return task;
        }

        protected override void Activate() => AbilitySystem.OnGameplayEvent += HandleEvent;

        void HandleEvent(GameplayTag tag, GameplayEventData payload)
        {
            if (!tag.MatchesTag(eventTag) || !ShouldBroadcastAbilityTaskDelegates) return;
            if (onlyTriggerOnce) EndTask();
            EventReceived?.Invoke(payload);
        }

        protected override void OnDestroy() => AbilitySystem.OnGameplayEvent -= HandleEvent;
    }

    /// <summary>UAbilityTask_WaitGameplayTagAdded / Removed in one: fires when the owner gains or loses a tag.</summary>
    public class AbilityTask_WaitGameplayTag : AbilityTask
    {
        GameplayTag tag;
        bool waitForAdded;
        public event Action OnTagChanged;

        public static AbilityTask_WaitGameplayTag WaitTagAdded(GameplayAbility owningAbility, GameplayTag tag) => Create(owningAbility, tag, true);
        public static AbilityTask_WaitGameplayTag WaitTagRemoved(GameplayAbility owningAbility, GameplayTag tag) => Create(owningAbility, tag, false);

        static AbilityTask_WaitGameplayTag Create(GameplayAbility ability, GameplayTag tag, bool added)
        {
            var task = ability.NewAbilityTask<AbilityTask_WaitGameplayTag>();
            task.tag = tag;
            task.waitForAdded = added;
            return task;
        }

        protected override void Activate()
        {
            if (AbilitySystem.HasMatchingGameplayTag(tag) == waitForAdded) { Fire(); return; }
            AbilitySystem.OwnedTags.RegisterGameplayTagEvent(tag, HandleTag);
        }

        void HandleTag(GameplayTag changed, int newCount)
        {
            if ((newCount > 0) == waitForAdded) Fire();
        }

        void Fire()
        {
            bool broadcast = ShouldBroadcastAbilityTaskDelegates;
            EndTask();
            if (broadcast) OnTagChanged?.Invoke();
        }

        protected override void OnDestroy() => AbilitySystem.OwnedTags.UnregisterGameplayTagEvent(tag, HandleTag);
    }
}
