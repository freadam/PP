using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// Every character's numbers. Damage is a meta attribute: effects add to it, and PostGameplayEffectExecute converts
    /// it into Health loss, so mitigation, death and on-damage events live in exactly one place.
    /// </summary>
    public class CharacterAttributeSet : AttributeSet
    {
        public static readonly GameplayAttribute Health = new GameplayAttribute("Health");
        public static readonly GameplayAttribute MaxHealth = new GameplayAttribute("MaxHealth");
        public static readonly GameplayAttribute Mana = new GameplayAttribute("Mana");
        public static readonly GameplayAttribute MaxMana = new GameplayAttribute("MaxMana");
        public static readonly GameplayAttribute AttackPower = new GameplayAttribute("AttackPower");
        public static readonly GameplayAttribute AbilityPower = new GameplayAttribute("AbilityPower");
        public static readonly GameplayAttribute Armor = new GameplayAttribute("Armor");
        public static readonly GameplayAttribute MoveSpeed = new GameplayAttribute("MoveSpeed");
        /// <summary>LoL-style ability haste: cooldown = base * 100 / (100 + haste).</summary>
        public static readonly GameplayAttribute AbilityHaste = new GameplayAttribute("AbilityHaste");
        /// <summary>Meta attribute: incoming damage, consumed immediately.</summary>
        public static readonly GameplayAttribute Damage = new GameplayAttribute("Damage");

        public CharacterAttributeSet()
        {
            DefineAttribute(Health, 100f);
            DefineAttribute(MaxHealth, 100f);
            DefineAttribute(Mana, 0f);
            DefineAttribute(MaxMana, 0f);
            DefineAttribute(AttackPower, 0f);
            DefineAttribute(AbilityPower, 0f);
            DefineAttribute(Armor, 0f);
            DefineAttribute(MoveSpeed, 5f);
            DefineAttribute(AbilityHaste, 0f);
            DefineAttribute(Damage, 0f);
        }

        public float GetHealth() => Get(Health);
        public float GetMaxHealth() => Get(MaxHealth);
        public float GetMana() => Get(Mana);
        public float GetMaxMana() => Get(MaxMana);

        public override void PreAttributeBaseChange(GameplayAttribute attribute, ref float newValue) => Clamp(attribute, ref newValue);
        public override void PreAttributeChange(GameplayAttribute attribute, ref float newValue) => Clamp(attribute, ref newValue);

        void Clamp(GameplayAttribute attribute, ref float value)
        {
            if (attribute == Health) value = Mathf.Clamp(value, 0f, Get(MaxHealth));
            else if (attribute == Mana) value = Mathf.Clamp(value, 0f, Get(MaxMana));
            else if (attribute == MaxHealth || attribute == MaxMana) value = Mathf.Max(1f, value);
            else if (attribute == MoveSpeed) value = Mathf.Max(0.5f, value);
        }

        public override void PostAttributeChange(GameplayAttribute attribute, float oldValue, float newValue)
        {
            // Gaining max health grants the difference (like levelling in LoL); losing it just re-clamps.
            if (attribute == MaxHealth)
                SetBase(Health, GetBase(Health) + Mathf.Max(0f, newValue - oldValue));
            else if (attribute == MaxMana)
                SetBase(Mana, GetBase(Mana) + Mathf.Max(0f, newValue - oldValue));
        }

        public override void PostGameplayEffectExecute(GameplayEffectModCallbackData data)
        {
            if (data.EvaluatedData.Attribute != Damage) return;

            float damage = GetBase(Damage);
            SetBase(Damage, 0f);
            if (damage <= 0f || Owner.HasMatchingGameplayTag(GameTags.State_Dead)) return;

            SetBase(Health, GetBase(Health) - damage);

            var context = data.EffectSpec.Context;
            var instigatorAvatar = context.Instigator != null ? context.Instigator.AvatarActor : null;
            Owner.HandleGameplayEvent(GameTags.Event_Damaged, new GameplayEventData
            {
                Instigator = instigatorAvatar,
                Target = Owner.AvatarActor,
                EventMagnitude = damage,
                Context = context,
                OptionalObject = data.EffectSpec,
            });

            if (GetBase(Health) <= 0f)
            {
                Owner.AddLooseGameplayTag(GameTags.State_Dead);
                if (context.Instigator != null && context.Instigator != Owner)
                {
                    context.Instigator.HandleGameplayEvent(GameTags.Event_Kill, new GameplayEventData
                    {
                        Instigator = instigatorAvatar,
                        Target = Owner.AvatarActor,
                        Context = context,
                    });
                }
            }
        }
    }
}
