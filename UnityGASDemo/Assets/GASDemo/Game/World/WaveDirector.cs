using System.Collections.Generic;
using UnityEngine;

namespace GASDemo
{
    /// <summary>Run structure: waves of enemies, a boon choice after each cleared wave, game over on death.</summary>
    public class WaveDirector : MonoBehaviour
    {
        public enum Phase { Intermission, Fighting, ChoosingBoon, Victory, GameOver }

        public DemoContent Content;
        public CharacterBase Hero;
        public int FinalWave = 8;
        public float IntermissionTime = 2f;

        public Phase CurrentPhase { get; private set; } = Phase.Intermission;
        public int Wave { get; private set; }
        public int Kills { get; private set; }
        public float PhaseTimer { get; private set; }
        public IReadOnlyList<Boon> BoonChoices => boonChoices;
        public int AliveEnemies => alive.Count;

        readonly List<CharacterBase> alive = new List<CharacterBase>();
        readonly List<Boon> boonChoices = new List<Boon>();
        List<Boon> boonPool;
        int pendingSpawns;

        void Start()
        {
            boonPool = Boon.CreatePool(Content);
            PhaseTimer = IntermissionTime;
            Hero.OnDied += _ => CurrentPhase = Phase.GameOver;
        }

        void Update()
        {
            switch (CurrentPhase)
            {
                case Phase.Intermission:
                    PhaseTimer -= Time.deltaTime;
                    if (PhaseTimer <= 0f) StartNextWave();
                    break;

                case Phase.Fighting:
                    alive.RemoveAll(e => e == null || !e.IsAlive);
                    if (alive.Count == 0 && pendingSpawns == 0) OnWaveCleared();
                    break;

                case Phase.ChoosingBoon:
                    for (int i = 0; i < boonChoices.Count; i++)
                        if (InputReader.Down(DemoButton.Choice1 + i)) ChooseBoon(i);
                    break;
            }
        }

        void StartNextWave()
        {
            Wave++;
            CurrentPhase = Phase.Fighting;

            int grunts = 3 + Wave * 2;
            int brutes = Wave / 2;
            int casters = Wave >= 2 ? Wave - 1 : 0;
            var kinds = new List<EnemyKind>();
            for (int i = 0; i < grunts; i++) kinds.Add(EnemyKind.Grunt);
            for (int i = 0; i < brutes; i++) kinds.Add(EnemyKind.Brute);
            for (int i = 0; i < casters; i++) kinds.Add(EnemyKind.Caster);

            for (int i = 0; i < kinds.Count; i++)
                StartCoroutine(SpawnAfter(kinds[i], 0.15f * i));
        }

        System.Collections.IEnumerator SpawnAfter(EnemyKind kind, float delay)
        {
            pendingSpawns++;
            yield return new WaitForSeconds(delay);

            var heroPos = Hero != null ? Hero.transform.position : Vector3.zero;
            var dir = Random.insideUnitCircle.normalized;
            var pos = heroPos + new Vector3(dir.x, 0f, dir.y) * Random.Range(9f, 14f);
            var flat = Targeting.Flat(pos);
            if (flat.magnitude > CharacterBase.ArenaRadius - 2f) pos = flat.normalized * (CharacterBase.ArenaRadius - 2f);

            var marker = Telegraphs.Create(pos, Quaternion.identity, 1f, 360f, 0.8f, new Color(1f, 0.2f, 0.2f, 0.35f));
            yield return new WaitForSeconds(0.8f);
            if (marker != null) Destroy(marker);

            if (CurrentPhase == Phase.Fighting)
            {
                var enemy = Content.SpawnEnemy(kind, pos, Wave);
                enemy.FaceTowards(heroPos);
                enemy.OnDied += _ => Kills++;
                alive.Add(enemy);
                Fx.Burst(pos + Vector3.up, new Color(1f, 0.3f, 0.2f, 0.6f), 0.5f, 2.5f, 0.25f);
            }
            pendingSpawns--;
        }

        void OnWaveCleared()
        {
            if (Wave >= FinalWave)
            {
                CurrentPhase = Phase.Victory;
                return;
            }

            boonChoices.Clear();
            boonChoices.AddRange(Boon.Roll(boonPool, Hero, 3));
            if (boonChoices.Count == 0)
            {
                BeginIntermission();
                return;
            }
            CurrentPhase = Phase.ChoosingBoon;
            Time.timeScale = 0f;
        }

        public void ChooseBoon(int index)
        {
            if (CurrentPhase != Phase.ChoosingBoon || index < 0 || index >= boonChoices.Count) return;
            boonChoices[index].Apply(Hero);
            FloatingText.Spawn(Hero.transform.position + Vector3.up * 2.8f, boonChoices[index].Title, new Color(1f, 0.85f, 0.3f));
            boonChoices.Clear();
            Time.timeScale = 1f;
            BeginIntermission();
        }

        void BeginIntermission()
        {
            CurrentPhase = Phase.Intermission;
            PhaseTimer = IntermissionTime;
        }

        void OnDestroy()
        {
            if (Time.timeScale == 0f) Time.timeScale = 1f;
        }
    }
}
