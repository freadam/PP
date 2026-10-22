using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// Damage meter for a dummy. Listens to Event.Damaged on its ASC (the same event that drives damage numbers),
    /// so whatever the attribute set decides was dealt (after armor) is what gets counted.
    /// Heals back to full after a few seconds without being hit, so a test can be repeated.
    /// </summary>
    [RequireComponent(typeof(CharacterBase))]
    public class TrainingDummy : MonoBehaviour
    {
        public float ResetAfter = 3f;

        public float LastHit { get; private set; }
        public float Total { get; private set; }
        public int Hits { get; private set; }
        public float Dps => Hits == 0 ? 0f : Total / Mathf.Max(1f, lastHitTime - firstHitTime);

        CharacterBase character;
        float firstHitTime, lastHitTime = -100f;

        void Awake()
        {
            character = GetComponent<CharacterBase>();
            character.AbilitySystem.OnGameplayEvent += OnEvent;
        }

        void OnEvent(GameplayTag tag, GameplayEventData data)
        {
            if (!tag.MatchesTag(GameTags.Event_Damaged)) return;
            if (Time.time - lastHitTime > ResetAfter)
            {
                Total = 0f;
                Hits = 0;
                firstHitTime = Time.time;
            }
            lastHitTime = Time.time;
            LastHit = data.EventMagnitude;
            Total += data.EventMagnitude;
            Hits++;
        }

        void Update()
        {
            if (!character.IsAlive || Hits == 0 || Time.time - lastHitTime < ResetAfter) return;
            Hits = 0;
            Total = 0f;
            character.RefillVitals();
        }

        /// <summary>Label shown over the dummy by the sandbox HUD.</summary>
        public string Describe()
        {
            var asc = character.AbilitySystem;
            string armor = $"armor {asc.GetNumericAttribute(CharacterAttributeSet.Armor):0}";
            if (Hits == 0) return $"<b>{character.DisplayName}</b>\n{armor}";
            return $"<b>{character.DisplayName}</b>\nlast {LastHit:0}  total {Total:0}\n{Hits} hits · {Dps:0} dps";
        }
    }
}
