using System;
using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// The damage formula, as a GAS execution calculation:
    ///   raw   = Data.Damage + Data.Ratio.AD * source AttackPower + Data.Ratio.AP * source AbilityPower
    ///   final = raw * stacks * 100 / (100 + target Armor)
    /// Abilities only say "base 60, 1.0 AD ratio"; the numbers that make it hurt live here.
    /// </summary>
    [Serializable]
    public class DamageExecution : GameplayEffectExecutionCalculation
    {
        [Tooltip("Armor scaling (1 = full armor, 0 = true damage).")]
        public float ArmorEffectiveness = 1f;

        public override void Execute(GameplayEffectCustomExecutionParameters p, GameplayEffectCustomExecutionOutput output)
        {
            var spec = p.Spec;
            float raw = spec.GetSetByCallerMagnitude(GameTags.Data_Damage, false)
                        + spec.GetSetByCallerMagnitude(GameTags.Data_ADRatio, false) * p.GetSourceAttribute(CharacterAttributeSet.AttackPower)
                        + spec.GetSetByCallerMagnitude(GameTags.Data_APRatio, false) * p.GetSourceAttribute(CharacterAttributeSet.AbilityPower);

            float armor = Mathf.Max(0f, p.GetTargetAttribute(CharacterAttributeSet.Armor) * ArmorEffectiveness);
            float mitigated = raw * p.StackCount * 100f / (100f + armor);

            if (mitigated > 0f)
                output.AddOutputModifier(CharacterAttributeSet.Damage, ModifierOp.Add, mitigated);
        }
    }
}
