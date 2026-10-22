using GAS;
using UnityEngine;

namespace GASDemo
{
    /// <summary>
    /// Builds the whole demo at runtime: arena, light, camera, hero, wave director and HUD.
    /// Drop it on an empty GameObject in an empty scene and press Play (or use Tools > GAS Demo > Create Demo Scene).
    /// </summary>
    public class DemoBootstrap : MonoBehaviour
    {
        public int FinalWave = 8;

        DemoContent content;
        GameObject worldRoot;

        void Start()
        {
            content = DemoContent.Build();
            BuildWorld();
        }

        void BuildWorld()
        {
            Time.timeScale = 1f;
            Targeting.Clear();
            FloatingText.Entries.Clear();
            DemoCues.RegisterAll();

            worldRoot = new GameObject("GASDemo_World");
            Fx.Root = worldRoot.transform;

            BuildArena();
            EnsureLight();

            var hero = content.SpawnHero(Vector3.zero);

            SetupCamera(hero.transform);

            var director = worldRoot.AddComponent<WaveDirector>();
            director.Content = content;
            director.Hero = hero;
            director.FinalWave = FinalWave;

            var hud = worldRoot.AddComponent<DemoHUD>();
            hud.Hero = hero;
            hud.Director = director;
            hud.Content = content;
            hud.RestartRequested = Restart;
        }

        void Restart()
        {
            Destroy(worldRoot);
            BuildWorld();
        }

        internal static void SetupCamera(Transform target)
        {
            var cam = Camera.main;
            if (cam == null)
            {
                var camGo = new GameObject("Main Camera") { tag = "MainCamera" };
                cam = camGo.AddComponent<Camera>();
            }
            cam.fieldOfView = 45f;
            cam.clearFlags = CameraClearFlags.SolidColor;
            cam.backgroundColor = new Color(0.06f, 0.06f, 0.09f);
            var follow = cam.GetComponent<TopDownCamera>();
            if (follow == null) follow = cam.gameObject.AddComponent<TopDownCamera>();
            follow.Target = target;
        }

        internal static void BuildArena()
        {
            float r = CharacterBase.ArenaRadius;
            var floor = Fx.Primitive(PrimitiveType.Cylinder, new Vector3(0f, -0.1f, 0f), new Vector3(r * 2f + 1f, 0.1f, r * 2f + 1f), new Color(0.2f, 0.21f, 0.25f));
            floor.name = "Arena";
            floor.GetComponent<Renderer>().shadowCastingMode = UnityEngine.Rendering.ShadowCastingMode.Off;

            // Inner rune ring and a fence of pillars so the play space reads at a glance.
            Fx.GroundMesh("ArenaRing", Fx.SectorMesh(r, 360f, r - 0.4f), Vector3.zero, Quaternion.identity, new Color(0.9f, 0.5f, 0.2f, 0.35f));
            Fx.GroundMesh("CenterRune", Fx.SectorMesh(3.2f, 360f, 2.9f), Vector3.zero, Quaternion.identity, new Color(0.5f, 0.7f, 1f, 0.2f));
            for (int i = 0; i < 24; i++)
            {
                float a = i / 24f * Mathf.PI * 2f;
                var pos = new Vector3(Mathf.Cos(a), 0f, Mathf.Sin(a)) * (r + 1.2f);
                Fx.Primitive(PrimitiveType.Cube, pos + Vector3.up * 1.2f, new Vector3(1f, 2.4f + (i % 3) * 0.8f, 1f), new Color(0.32f, 0.3f, 0.35f)).name = "Pillar";
            }
        }

        internal static void EnsureLight()
        {
            if (Object.FindFirstObjectByType<Light>() != null) return;
            // Not parented to the world root: it must survive restarts (the old one is still alive during rebuild).
            var lightGo = new GameObject("Sun");
            lightGo.transform.rotation = Quaternion.Euler(55f, -35f, 0f);
            var light = lightGo.AddComponent<Light>();
            light.type = LightType.Directional;
            light.intensity = 1.15f;
            light.shadows = LightShadows.Soft;
            RenderSettings.ambientLight = new Color(0.35f, 0.36f, 0.42f);
        }
    }
}
