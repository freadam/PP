using System.Collections.Generic;

namespace GAS
{
    /// <summary>
    /// UAttributeSet. Declares attributes and owns the rules for them (clamping, meta attributes like Damage).
    /// Values are only ever changed through the owning <see cref="AbilitySystemComponent"/> so that
    /// aggregation, clamping and change notifications always run.
    /// </summary>
    public abstract class AttributeSet
    {
        readonly Dictionary<GameplayAttribute, GameplayAttributeData> attributes = new Dictionary<GameplayAttribute, GameplayAttributeData>();

        public AbilitySystemComponent Owner { get; internal set; }
        public IEnumerable<GameplayAttribute> Attributes => attributes.Keys;

        protected void DefineAttribute(GameplayAttribute attribute, float initialValue)
        {
            attributes[attribute] = new GameplayAttributeData(initialValue);
        }

        public bool HasAttribute(GameplayAttribute attribute) => attributes.ContainsKey(attribute);

        internal bool TryGetData(GameplayAttribute attribute, out GameplayAttributeData data) => attributes.TryGetValue(attribute, out data);

        // Convenience accessors for subclasses, so they read like UE's ATTRIBUTE_ACCESSORS.
        protected float Get(GameplayAttribute attribute) => Owner.GetNumericAttribute(attribute);
        protected float GetBase(GameplayAttribute attribute) => Owner.GetNumericAttributeBase(attribute);
        protected void SetBase(GameplayAttribute attribute, float value) => Owner.SetNumericAttributeBase(attribute, value);

        /// <summary>Clamp/adjust a new CurrentValue before it is stored (does not change the base).</summary>
        public virtual void PreAttributeChange(GameplayAttribute attribute, ref float newValue) { }

        /// <summary>Clamp/adjust a new BaseValue before it is stored.</summary>
        public virtual void PreAttributeBaseChange(GameplayAttribute attribute, ref float newValue) { }

        public virtual void PostAttributeChange(GameplayAttribute attribute, float oldValue, float newValue) { }

        /// <summary>Return false to veto an instant modification to the base value.</summary>
        public virtual bool PreGameplayEffectExecute(GameplayEffectModCallbackData data) => true;

        /// <summary>Runs after an instant/periodic modification. The place to turn meta attributes (Damage) into real ones (Health).</summary>
        public virtual void PostGameplayEffectExecute(GameplayEffectModCallbackData data) { }
    }

    /// <summary>FGameplayEffectModCallbackData</summary>
    public class GameplayEffectModCallbackData
    {
        public GameplayEffectSpec EffectSpec;
        public GameplayModifierEvaluatedData EvaluatedData;
        public AbilitySystemComponent Target;
    }
}
