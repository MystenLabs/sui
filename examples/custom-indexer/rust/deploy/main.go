package main

import (
	"fmt"

	common "github.com/MystenLabs/sui-operations/pulumi/common-go"
	gcp "github.com/pulumi/pulumi-gcp/sdk/v7/go/gcp"
	"github.com/pulumi/pulumi-gcp/sdk/v7/go/gcp/serviceaccount"
	"github.com/pulumi/pulumi-gcp/sdk/v7/go/gcp/storage"
	k8s "github.com/pulumi/pulumi-kubernetes/sdk/v4/go/kubernetes"
	batchv1 "github.com/pulumi/pulumi-kubernetes/sdk/v4/go/kubernetes/batch/v1"
	corev1 "github.com/pulumi/pulumi-kubernetes/sdk/v4/go/kubernetes/core/v1"
	metav1 "github.com/pulumi/pulumi-kubernetes/sdk/v4/go/kubernetes/meta/v1"
	"github.com/pulumi/pulumi/sdk/v3/go/pulumi"
	"github.com/pulumi/pulumi/sdk/v3/go/pulumi/config"
)

type DeployConfig struct {
	Namespace                    string          `json:"namespace"`
	BucketProject                string          `json:"bucket_project"`
	WorkloadIdentityProject      string          `json:"workload_identity_project"`
	GCPServiceAccountID          string          `json:"gcp_service_account_id"`
	KubernetesServiceAccountName string          `json:"kubernetes_service_account_name"`
	CheckpointBucket             string          `json:"checkpoint_bucket"`
	CheckpointStorageRole        string          `json:"checkpoint_storage_role"`
	OutputGCSBucket              string          `json:"output_gcs_bucket"`
	OutputGCSKey                 string          `json:"output_gcs_key"`
	StartCheckpoint              string          `json:"start_checkpoint"`
	EndCheckpoint                string          `json:"end_checkpoint"`
	Concurrency                  string          `json:"concurrency"`
	Resources                    ResourcesConfig `json:"resources"`
}

type ResourcesConfig struct {
	Requests ResourceValues `json:"requests"`
	Limits   ResourceValues `json:"limits"`
}

type ResourceValues struct {
	CPU    string `json:"cpu"`
	Memory string `json:"memory"`
}

func main() {
	pulumi.Run(func(ctx *pulumi.Context) error {
		cfg := config.New(ctx, "")

		var deployCfg DeployConfig
		cfg.RequireObject("deploy-config", &deployCfg)
		deployCfg = deployCfg.withDefaults(ctx.Project())
		if err := deployCfg.validate(); err != nil {
			return err
		}

		var image common.FetchOrBuild
		cfg.RequireObject("image", &image)
		image = image.WithDefaults()

		provider, err := common.GetK8sProviderFromESC(ctx, &common.K8sProviderOptions{
			Region: common.K8sProviderRegionUsEast4,
		})
		if err != nil {
			return err
		}

		imageURI, err := buildImage(ctx, image)
		if err != nil {
			return err
		}

		snapshotProvider, err := gcp.NewProvider(ctx, "fullnode-snapshot-gcs-provider", &gcp.ProviderArgs{
			Project: pulumi.String(deployCfg.BucketProject),
		})
		if err != nil {
			return err
		}

		ns, err := createNamespace(ctx, provider, deployCfg.Namespace)
		if err != nil {
			return err
		}

		gsa, err := createGCPServiceAccount(ctx, snapshotProvider, deployCfg)
		if err != nil {
			return err
		}

		ksa, err := createKubernetesServiceAccount(ctx, provider, ns, deployCfg, gsa)
		if err != nil {
			return err
		}

		dependencies := []pulumi.Resource{ns, ksa}

		wiBinding, err := bindWorkloadIdentity(ctx, snapshotProvider, deployCfg, gsa)
		if err != nil {
			return err
		}
		dependencies = append(dependencies, wiBinding)

		readBinding, err := grantBucketAccess(ctx, snapshotProvider,
			"checkpoint-read", deployCfg.CheckpointBucket, deployCfg.CheckpointStorageRole, gsa)
		if err != nil {
			return err
		}
		dependencies = append(dependencies, readBinding)

		// Output bucket may be the same as checkpoint bucket (different prefix).
		// Grant write access separately so it is explicit.
		writeBinding, err := grantBucketAccess(ctx, snapshotProvider,
			"output-write", deployCfg.OutputGCSBucket, "roles/storage.objectAdmin", gsa)
		if err != nil {
			return err
		}
		dependencies = append(dependencies, writeBinding)

		job, err := createJob(ctx, provider, ksa, deployCfg, imageURI, dependencies)
		if err != nil {
			return err
		}

		ctx.Export("namespace", ns.Metadata.Name())
		ctx.Export("serviceAccount", ksa.Metadata.Name())
		ctx.Export("gcpServiceAccount", gsa.Email)
		ctx.Export("job", job.Metadata.Name())
		ctx.Export("outputGCSPath", pulumi.Sprintf("gs://%s/%s", deployCfg.OutputGCSBucket, deployCfg.OutputGCSKey))
		ctx.Export("downloadCommand", pulumi.Sprintf("gsutil cp gs://%s/%s ./mismatches.jsonl", deployCfg.OutputGCSBucket, deployCfg.OutputGCSKey))

		return nil
	})
}

func (c DeployConfig) withDefaults(project string) DeployConfig {
	if c.Namespace == "" {
		c.Namespace = project
	}
	if c.BucketProject == "" {
		c.BucketProject = "fullnode-snapshot-gcs"
	}
	if c.WorkloadIdentityProject == "" {
		c.WorkloadIdentityProject = "workloads-primary"
	}
	if c.GCPServiceAccountID == "" {
		c.GCPServiceAccountID = "sig-order-scanner"
	}
	if c.KubernetesServiceAccountName == "" {
		c.KubernetesServiceAccountName = "sig-order-scanner"
	}
	if c.CheckpointBucket == "" {
		c.CheckpointBucket = "mysten-mainnet-checkpoints"
	}
	if c.CheckpointStorageRole == "" {
		c.CheckpointStorageRole = "roles/storage.objectViewer"
	}
	if c.OutputGCSKey == "" {
		c.OutputGCSKey = "sig-order-scan/results.jsonl"
	}
	if c.Concurrency == "" {
		c.Concurrency = "300"
	}
	if c.Resources.Requests.CPU == "" {
		c.Resources.Requests.CPU = "4"
	}
	if c.Resources.Requests.Memory == "" {
		c.Resources.Requests.Memory = "8Gi"
	}
	if c.Resources.Limits.CPU == "" {
		c.Resources.Limits.CPU = "8"
	}
	if c.Resources.Limits.Memory == "" {
		c.Resources.Limits.Memory = "16Gi"
	}
	return c
}

func (c DeployConfig) validate() error {
	if c.CheckpointBucket == "" {
		return fmt.Errorf("deploy-config.checkpoint_bucket must not be empty")
	}
	if c.OutputGCSBucket == "" {
		return fmt.Errorf("deploy-config.output_gcs_bucket must not be empty")
	}
	if c.StartCheckpoint == "" {
		return fmt.Errorf("deploy-config.start_checkpoint must not be empty")
	}
	if c.EndCheckpoint == "" {
		return fmt.Errorf("deploy-config.end_checkpoint must not be empty")
	}
	return nil
}

func buildImage(ctx *pulumi.Context, image common.FetchOrBuild) (string, error) {
	repoRef := image.RepoRef
	if repoRef == "" {
		repoRef = image.ImageRef
	}
	retryLimit := image.TryLimit
	imageArgs := common.GenerateImageArg{
		OrgName:     image.OrgName,
		RepoName:    image.RepoName,
		RepoRefType: image.RepoRefType,
		RepoRef:     repoRef,
		ImageNamesToMetadata: map[string]common.ImageBuildParams{
			image.ImageName: {
				DockerfilePath: image.Dockerfile,
				ExtraTags:      "",
				BuildArgs:      image.BuildArgs,
				BuildContext:   image.Context,
			},
		},
		Force:       image.Force,
		TryInterval: &image.TryInterval,
		TryLimit:    &retryLimit,
		Context:     image.Context,
	}
	imageNameToSha, err := common.GenerateImageNameToShaMap(ctx, &imageArgs)
	if err != nil {
		return "", err
	}
	imageSHA, ok := imageNameToSha[image.ImageName]
	if !ok {
		return "", fmt.Errorf("image builder did not return a sha for image %q", image.ImageName)
	}
	return fmt.Sprintf(
		"%s/%s/%s/%s:%s",
		image.GCPArtifactRegistryTLD,
		image.GCPProject,
		image.RepoName,
		image.ImageName,
		imageSHA,
	), nil
}

func createNamespace(ctx *pulumi.Context, provider *k8s.Provider, namespace string) (*corev1.Namespace, error) {
	return corev1.NewNamespace(ctx, "namespace", &corev1.NamespaceArgs{
		Metadata: &metav1.ObjectMetaArgs{
			Name: pulumi.String(namespace),
			Labels: pulumi.StringMap{
				"app.kubernetes.io/name":       pulumi.String(ctx.Project()),
				"app.kubernetes.io/managed-by": pulumi.String("pulumi"),
			},
		},
	}, pulumi.Provider(provider))
}

func createGCPServiceAccount(ctx *pulumi.Context, provider *gcp.Provider, deployCfg DeployConfig) (*serviceaccount.Account, error) {
	return serviceaccount.NewAccount(ctx, "gcp-service-account", &serviceaccount.AccountArgs{
		AccountId:   pulumi.String(deployCfg.GCPServiceAccountID),
		DisplayName: pulumi.String("sig-order-scanner"),
		Description: pulumi.String("Reads mainnet checkpoints from GCS and writes scan results."),
		Project:     pulumi.String(deployCfg.BucketProject),
	}, pulumi.Provider(provider))
}

func createKubernetesServiceAccount(
	ctx *pulumi.Context,
	provider *k8s.Provider,
	ns *corev1.Namespace,
	deployCfg DeployConfig,
	gsa *serviceaccount.Account,
) (*corev1.ServiceAccount, error) {
	return corev1.NewServiceAccount(ctx, "kubernetes-service-account", &corev1.ServiceAccountArgs{
		Metadata: &metav1.ObjectMetaArgs{
			Name:      pulumi.String(deployCfg.KubernetesServiceAccountName),
			Namespace: ns.Metadata.Name(),
			Labels:    labels(ctx.Project()),
			Annotations: pulumi.StringMap{
				"iam.gke.io/gcp-service-account": gsa.Email,
			},
		},
	}, pulumi.Provider(provider), pulumi.DependsOn([]pulumi.Resource{ns}))
}

func bindWorkloadIdentity(
	ctx *pulumi.Context,
	provider *gcp.Provider,
	deployCfg DeployConfig,
	gsa *serviceaccount.Account,
) (*serviceaccount.IAMMember, error) {
	return serviceaccount.NewIAMMember(ctx, "workload-identity-binding", &serviceaccount.IAMMemberArgs{
		ServiceAccountId: gsa.Name,
		Role:             pulumi.String("roles/iam.workloadIdentityUser"),
		Member: pulumi.String(fmt.Sprintf(
			"serviceAccount:%s.svc.id.goog[%s/%s]",
			deployCfg.WorkloadIdentityProject,
			deployCfg.Namespace,
			deployCfg.KubernetesServiceAccountName,
		)),
	}, pulumi.Provider(provider))
}

func grantBucketAccess(
	ctx *pulumi.Context,
	provider *gcp.Provider,
	name string,
	bucket string,
	role string,
	gsa *serviceaccount.Account,
) (*storage.BucketIAMMember, error) {
	return storage.NewBucketIAMMember(ctx, name, &storage.BucketIAMMemberArgs{
		Bucket: pulumi.String(bucket),
		Role:   pulumi.String(role),
		Member: pulumi.Sprintf("serviceAccount:%s", gsa.Email),
	}, pulumi.Provider(provider))
}

func createJob(
	ctx *pulumi.Context,
	provider *k8s.Provider,
	ksa *corev1.ServiceAccount,
	deployCfg DeployConfig,
	imageURI string,
	dependencies []pulumi.Resource,
) (*batchv1.Job, error) {
	appLabels := labels(ctx.Project())
	var backoffLimit int = 0
	var deadline int64 = 172800 // 48 hours

	return batchv1.NewJob(ctx, "job", &batchv1.JobArgs{
		Metadata: &metav1.ObjectMetaArgs{
			Name:      pulumi.String(ctx.Project()),
			Namespace: pulumi.String(deployCfg.Namespace),
			Labels:    appLabels,
		},
		Spec: &batchv1.JobSpecArgs{
			BackoffLimit:          pulumi.Int(backoffLimit),
			ActiveDeadlineSeconds: pulumi.Int64(deadline),
			Template: &corev1.PodTemplateSpecArgs{
				Metadata: &metav1.ObjectMetaArgs{
					Labels: appLabels,
					Annotations: pulumi.StringMap{
						"tags.datadoghq.com/env":     pulumi.String(ctx.Stack()),
						"tags.datadoghq.com/service": pulumi.String(ctx.Project()),
					},
				},
				Spec: &corev1.PodSpecArgs{
					ServiceAccountName: ksa.Metadata.Name(),
					RestartPolicy:      pulumi.String("Never"),
					Containers: corev1.ContainerArray{
						corev1.ContainerArgs{
							Name:            pulumi.String(ctx.Project()),
							Image:           pulumi.String(imageURI),
							ImagePullPolicy: pulumi.String("Always"),
							Env: corev1.EnvVarArray{
								corev1.EnvVarArgs{Name: pulumi.String("GCS_BUCKET"), Value: pulumi.String(deployCfg.CheckpointBucket)},
								corev1.EnvVarArgs{Name: pulumi.String("START_CHECKPOINT"), Value: pulumi.String(deployCfg.StartCheckpoint)},
								corev1.EnvVarArgs{Name: pulumi.String("END_CHECKPOINT"), Value: pulumi.String(deployCfg.EndCheckpoint)},
								corev1.EnvVarArgs{Name: pulumi.String("CONCURRENCY"), Value: pulumi.String(deployCfg.Concurrency)},
								corev1.EnvVarArgs{Name: pulumi.String("OUTPUT_FILE"), Value: pulumi.String("/data/mismatches.jsonl")},
								corev1.EnvVarArgs{Name: pulumi.String("OUTPUT_GCS_BUCKET"), Value: pulumi.String(deployCfg.OutputGCSBucket)},
								corev1.EnvVarArgs{Name: pulumi.String("OUTPUT_GCS_KEY"), Value: pulumi.String(deployCfg.OutputGCSKey)},
							},
							VolumeMounts: corev1.VolumeMountArray{
								corev1.VolumeMountArgs{
									Name:      pulumi.String("data"),
									MountPath: pulumi.String("/data"),
								},
							},
							Resources: &corev1.ResourceRequirementsArgs{
								Requests: pulumi.StringMap{
									"cpu":    pulumi.String(deployCfg.Resources.Requests.CPU),
									"memory": pulumi.String(deployCfg.Resources.Requests.Memory),
								},
								Limits: pulumi.StringMap{
									"cpu":    pulumi.String(deployCfg.Resources.Limits.CPU),
									"memory": pulumi.String(deployCfg.Resources.Limits.Memory),
								},
							},
						},
					},
					Volumes: corev1.VolumeArray{
						corev1.VolumeArgs{
							Name: pulumi.String("data"),
							EmptyDir: &corev1.EmptyDirVolumeSourceArgs{
								SizeLimit: pulumi.String("20Gi"),
							},
						},
					},
				},
			},
		},
	}, pulumi.Provider(provider), pulumi.DependsOn(dependencies))
}

func labels(app string) pulumi.StringMap {
	return pulumi.StringMap{
		"app.kubernetes.io/name":      pulumi.String(app),
		"app.kubernetes.io/component": pulumi.String("sig-order-scanner"),
	}
}
