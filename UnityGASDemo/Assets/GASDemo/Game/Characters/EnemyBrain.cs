using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// Minimal AI controller. Like a BT task in UE it only decides *when*; it asks the ASC to
    /// "activate something tagged Ability.Enemy.Attack" and the ability handles telegraph, timing and damage.
    /// </summary>
    [RequireComponent(typeof(CharacterBase))]
    public class EnemyBrain : MonoBehaviour
    {
        public float AttackRange = 2f;
        [Tooltip("Ranged enemies back off when the target is closer than this.")]
        public float KeepAwayDistance;
        public float ThinkDelay = 0.8f;

        static readonly GameplayTagContainer AttackTags = GameplayTagContainer.Of(GameTags.Ability_Enemy_Attack);

        CharacterBase character;
        float wakeTimer;

        void Awake()
        {
            character = GetComponent<CharacterBase>();
            wakeTimer = ThinkDelay + Random.Range(0f, 0.4f);
        }

        void Update()
        {
            character.MoveInput = Vector3.zero;
            if (!character.IsAlive) return;
            if (wakeTimer > 0f) { wakeTimer -= Time.deltaTime; return; }

            // Busy (winding up, recovering) or crowd-controlled: the ability or the effect owns the body right now.
            var asc = character.AbilitySystem;
            if (asc.HasMatchingGameplayTag(GameTags.State_Casting) || asc.HasMatchingGameplayTag(GameTags.State_Debuff_Stun)) return;

            var target = Targeting.FindNearestHostile(character.Team, transform.position);
            if (target == null) return;

            var to = Targeting.Flat(target.transform.position - transform.position);
            float dist = to.magnitude;
            character.AimPoint = target.transform.position;

            if (dist <= AttackRange + target.Radius)
            {
                character.FaceTowards(target.transform.position);
                if (KeepAwayDistance > 0f && dist < KeepAwayDistance)
                    character.MoveInput = -to.normalized * 0.8f;
                asc.TryActivateAbilitiesByTag(AttackTags);
            }
            else
            {
                character.MoveInput = to.normalized;
            }
        }
    }
}
