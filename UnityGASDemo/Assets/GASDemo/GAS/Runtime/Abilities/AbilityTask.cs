namespace GAS
{
    /// <summary>
    /// UAbilityTask: an asynchronous step inside an ability (wait, wait for input, wait for event, dash...).
    /// Tasks are owned by one activation; ending the ability ends all of its tasks, so delegates never fire
    /// into an ability that is no longer running.
    ///
    /// Usage mirrors UE:  var t = AbilityTask_WaitDelay.WaitDelay(this, 0.3f); t.OnFinish += Fire; t.ReadyForActivation();
    /// </summary>
    public abstract class AbilityTask
    {
        public GameplayAbility Ability { get; private set; }
        public bool IsFinished { get; private set; }
        public virtual bool IsTickingTask => false;

        protected AbilitySystemComponent AbilitySystem => Ability.AbilitySystem;

        internal void InitTask(GameplayAbility ability) => Ability = ability;

        public void ReadyForActivation()
        {
            if (!IsFinished) Activate();
        }

        protected virtual void Activate() { }
        public virtual void TickTask(float deltaTime) { }
        public virtual void OnInputPressed() { }
        public virtual void OnInputReleased() { }

        /// <summary>Guard delegate broadcasts with this, like UE's ShouldBroadcastAbilityTaskDelegates().</summary>
        protected bool ShouldBroadcastAbilityTaskDelegates => !IsFinished && Ability != null && Ability.IsActive;

        public void EndTask()
        {
            if (IsFinished) return;
            IsFinished = true;
            OnDestroy();
            Ability.OnTaskEnded(this);
        }

        protected virtual void OnDestroy() { }
    }
}
