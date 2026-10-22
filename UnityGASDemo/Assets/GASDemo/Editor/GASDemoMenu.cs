using System.IO;
using GAS;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

namespace GASDemo.Editor
{
    public static class GASDemoMenu
    {
        const string SceneFolder = "Assets/GASDemo/Scenes";
        const string ContentFolder = "Assets/GASDemo/Content";

        [MenuItem("Tools/GAS Demo/Create Demo Scene")]
        public static void CreateDemoScene() => CreateScene<DemoBootstrap>("GASDemo");

        [MenuItem("Tools/GAS Demo/Create Ability Sandbox Scene")]
        public static void CreateSandboxScene() => CreateScene<SandboxBootstrap>("GASDemo_Sandbox");

        static void CreateScene<T>(string sceneName) where T : MonoBehaviour
        {
            if (!EditorSceneManager.SaveCurrentModifiedScenesIfUserWantsTo()) return;

            var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
            new GameObject(sceneName).AddComponent<T>();

            Directory.CreateDirectory(SceneFolder);
            string path = $"{SceneFolder}/{sceneName}.unity";
            EditorSceneManager.SaveScene(scene, path);
            AssetDatabase.Refresh();
            Debug.Log($"[GAS Demo] Scene created at {path}. Press Play.");
        }

        /// <summary>
        /// Saves every effect and ability the demo builds in code as real .asset files, so they can be tuned in the
        /// inspector — the same objects a designer would author for UE's GAS as GE_/GA_ Blueprints.
        /// </summary>
        [MenuItem("Tools/GAS Demo/Export Content As Assets")]
        public static void ExportContent()
        {
            Directory.CreateDirectory(ContentFolder + "/Effects");
            Directory.CreateDirectory(ContentFolder + "/Abilities");

            var content = DemoContent.Build();
            foreach (var asset in content.AllAssets)
            {
                string sub = asset is GameplayEffect ? "Effects" : "Abilities";
                string path = AssetDatabase.GenerateUniqueAssetPath($"{ContentFolder}/{sub}/{asset.name}.asset");
                AssetDatabase.CreateAsset(asset, path);
            }
            AssetDatabase.SaveAssets();
            AssetDatabase.Refresh();
            Debug.Log($"[GAS Demo] Exported {content.AllAssets.Count} assets to {ContentFolder}.");
        }
    }
}
