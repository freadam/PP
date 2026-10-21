using System;
using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>Moves the avatar a fixed distance over time (UE: AbilityTask_ApplyRootMotionConstantForce).</summary>
    public class AbilityTask_Dash : AbilityTask
    {
        CharacterBase avatar;
        Vector3 direction;
        float speed, remaining, afterimageTimer;
        Color trailColor;

        public event Action OnFinish;
        public override bool IsTickingTask => true;

        public static AbilityTask_Dash Dash(GameplayAbility ability, CharacterBase avatar, Vector3 direction, float distance, float duration, Color trailColor)
        {
            var task = ability.NewAbilityTask<AbilityTask_Dash>();
            task.avatar = avatar;
            task.direction = Targeting.Flat(direction).normalized;
            task.remaining = Mathf.Max(0.01f, duration);
            task.speed = distance / task.remaining;
            task.trailColor = trailColor;
            return task;
        }

        protected override void Activate()
        {
            if (direction.sqrMagnitude > 0f) avatar.transform.rotation = Quaternion.LookRotation(direction, Vector3.up);
        }

        public override void TickTask(float deltaTime)
        {
            float dt = Mathf.Min(deltaTime, remaining);
            avatar.Move(direction * speed * dt);
            remaining -= dt;

            afterimageTimer -= deltaTime;
            if (afterimageTimer <= 0f)
            {
                afterimageTimer = 0.03f;
                Fx.Burst(avatar.transform.position + Vector3.up, trailColor, 0.9f, 0.5f, 0.25f, PrimitiveType.Capsule);
            }

            if (remaining > 0f) return;
            bool broadcast = ShouldBroadcastAbilityTaskDelegates;
            EndTask();
            if (broadcast) OnFinish?.Invoke();
        }
    }
}
