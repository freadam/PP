using System;
using UnityEngine;

namespace GAS
{
    /// <summary>FScalableFloat: a value that scales with the effect/ability level. Value + PerLevel * (level - 1).</summary>
    [Serializable]
    public struct ScalableFloat
    {
        public float Value;
        public float PerLevel;

        public ScalableFloat(float value, float perLevel = 0f)
        {
            Value = value;
            PerLevel = perLevel;
        }

        public float GetValueAtLevel(float level) => Value + PerLevel * Mathf.Max(0f, level - 1f);

        public static implicit operator ScalableFloat(float value) => new ScalableFloat(value);
    }

    /// <summary>
    /// EGameplayModOp. Aggregation follows UE: (Base + ΣAdd) * (1 + Σ(Multiply - 1)), unless an Override is present.
    /// Multiply 1.2 therefore means "+20%", and two +20% modifiers give +40% (not +44%).
    /// </summary>
    public enum ModifierOp { Add, Multiply, Override }

    public enum MagnitudeCalculationType { ScalableFloat, AttributeBased, SetByCaller }

    public enum AttributeCaptureSource { Source, Target }

    /// <summary>FAttributeBasedFloat: Coefficient * (Attribute + PreMultiplyAdditive) + PostMultiplyAdditive.</summary>
    [Serializable]
    public class AttributeBasedFloat
    {
        public GameplayAttribute BackingAttribute;
        public AttributeCaptureSource AttributeSource = AttributeCaptureSource.Source;
        public ScalableFloat Coefficient = new ScalableFloat(1f);
        public float PreMultiplyAdditiveValue;
        public float PostMultiplyAdditiveValue;
    }

    /// <summary>FGameplayEffectModifierMagnitude</summary>
    [Serializable]
    public class GameplayEffectModifierMagnitude
    {
        public MagnitudeCalculationType CalculationType = MagnitudeCalculationType.ScalableFloat;
        public ScalableFloat ScalableFloatMagnitude;
        public AttributeBasedFloat AttributeBasedMagnitude = new AttributeBasedFloat();
        public GameplayTag SetByCallerTag;

        public static GameplayEffectModifierMagnitude Scalable(float value, float perLevel = 0f) =>
            new GameplayEffectModifierMagnitude { CalculationType = MagnitudeCalculationType.ScalableFloat, ScalableFloatMagnitude = new ScalableFloat(value, perLevel) };

        public static GameplayEffectModifierMagnitude SetByCaller(GameplayTag tag) =>
            new GameplayEffectModifierMagnitude { CalculationType = MagnitudeCalculationType.SetByCaller, SetByCallerTag = tag };

        public static GameplayEffectModifierMagnitude AttributeBased(GameplayAttribute attribute, AttributeCaptureSource source,
            float coefficient = 1f, float preAdd = 0f, float postAdd = 0f) =>
            new GameplayEffectModifierMagnitude
            {
                CalculationType = MagnitudeCalculationType.AttributeBased,
                AttributeBasedMagnitude = new AttributeBasedFloat
                {
                    BackingAttribute = attribute, AttributeSource = source, Coefficient = coefficient,
                    PreMultiplyAdditiveValue = preAdd, PostMultiplyAdditiveValue = postAdd,
                },
            };

        public float Calculate(GameplayEffectSpec spec, AbilitySystemComponent target)
        {
            switch (CalculationType)
            {
                case MagnitudeCalculationType.ScalableFloat:
                    return ScalableFloatMagnitude.GetValueAtLevel(spec.Level);

                case MagnitudeCalculationType.SetByCaller:
                    return spec.GetSetByCallerMagnitude(SetByCallerTag);

                case MagnitudeCalculationType.AttributeBased:
                {
                    var a = AttributeBasedMagnitude;
                    var asc = a.AttributeSource == AttributeCaptureSource.Source ? spec.Context.Instigator : target;
                    float captured = asc != null ? asc.GetNumericAttribute(a.BackingAttribute) : 0f;
                    return a.Coefficient.GetValueAtLevel(spec.Level) * (captured + a.PreMultiplyAdditiveValue) + a.PostMultiplyAdditiveValue;
                }
            }
            return 0f;
        }
    }

    /// <summary>FGameplayModifierInfo: "modify Attribute with Op by Magnitude".</summary>
    [Serializable]
    public class GameplayModifierInfo
    {
        public GameplayAttribute Attribute;
        public ModifierOp Op = ModifierOp.Add;
        public GameplayEffectModifierMagnitude Magnitude = new GameplayEffectModifierMagnitude();

        public GameplayModifierInfo() { }

        public GameplayModifierInfo(GameplayAttribute attribute, ModifierOp op, GameplayEffectModifierMagnitude magnitude)
        {
            Attribute = attribute;
            Op = op;
            Magnitude = magnitude;
        }
    }

    /// <summary>FGameplayModifierEvaluatedData: a modifier with its magnitude already resolved.</summary>
    public struct GameplayModifierEvaluatedData
    {
        public GameplayAttribute Attribute;
        public ModifierOp Op;
        public float Magnitude;

        public GameplayModifierEvaluatedData(GameplayAttribute attribute, ModifierOp op, float magnitude)
        {
            Attribute = attribute;
            Op = op;
            Magnitude = magnitude;
        }
    }
}
