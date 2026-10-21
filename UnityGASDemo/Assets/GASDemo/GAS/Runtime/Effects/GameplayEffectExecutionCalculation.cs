using System;
using System.Collections.Generic;

namespace GAS
{
    /// <summary>
    /// UGameplayEffectExecutionCalculation: arbitrary code that reads source/target attributes and outputs modifiers.
    /// The standard place for damage formulas (attack power, armor, crits...). Runs only for instant and periodic
    /// executions. Serialized by reference so an effect asset can list several, each with its own settings.
    /// </summary>
    [Serializable]
    public abstract class GameplayEffectExecutionCalculation
    {
        public abstract void Execute(GameplayEffectCustomExecutionParameters execParams, GameplayEffectCustomExecutionOutput output);
    }

    public class GameplayEffectCustomExecutionParameters
    {
        public GameplayEffectSpec Spec { get; }
        public AbilitySystemComponent Source => Spec.Context.Instigator;
        public AbilitySystemComponent Target { get; }
        public int StackCount { get; }

        public GameplayEffectCustomExecutionParameters(GameplayEffectSpec spec, AbilitySystemComponent target, int stackCount)
        {
            Spec = spec;
            Target = target;
            StackCount = stackCount;
        }

        public float GetSourceAttribute(GameplayAttribute attribute) => Source != null ? Source.GetNumericAttribute(attribute) : 0f;
        public float GetTargetAttribute(GameplayAttribute attribute) => Target != null ? Target.GetNumericAttribute(attribute) : 0f;
    }

    public class GameplayEffectCustomExecutionOutput
    {
        public readonly List<GameplayModifierEvaluatedData> OutputModifiers = new List<GameplayModifierEvaluatedData>();

        public void AddOutputModifier(GameplayAttribute attribute, ModifierOp op, float magnitude) =>
            OutputModifiers.Add(new GameplayModifierEvaluatedData(attribute, op, magnitude));
    }
}
