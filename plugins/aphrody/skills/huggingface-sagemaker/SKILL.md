---
name: huggingface-sagemaker
description: "Deploy Hugging Face models to Amazon SageMaker: AWS context discovery, IAM preflight, serving image, production defaults, Python env. Use for any SageMaker endpoint work."
---
# Hugging Face on SageMaker

Run the references in this order: context discovery, python env, IAM preflight, serving image, deployment planner, production defaults.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/hf-cloud-aws-context-discovery.md](references/hf-cloud-aws-context-discovery.md) | Discover the user's local AWS context (active profile, region, account ID, caller identity) at the start. Use before any other AWS work — deploying to SageMaker, creating resources, calling AWS APIs |
| [references/hf-cloud-python-env-setup.md](references/hf-cloud-python-env-setup.md) | Set up an isolated Python environment for SageMaker / AWS work, with the right Python version and current boto3. Use whenever Python code will be executed for a SageMaker deployment, training job |
| [references/hf-cloud-sagemaker-deployment-planner.md](references/hf-cloud-sagemaker-deployment-planner.md) | Plan and coordinate the deployment of a model to Amazon SageMaker AI. Use whenever the user wants to deploy, host, serve, or expose a model on SageMaker or AWS — including phrases like "deploy |
| [references/hf-cloud-sagemaker-iam-preflight.md](references/hf-cloud-sagemaker-iam-preflight.md) | Ensure a usable SageMaker execution role exists before deploying or training. Use whenever about to create a SageMaker endpoint, model, training job |
| [references/hf-cloud-sagemaker-production-defaults.md](references/hf-cloud-sagemaker-production-defaults.md) | Create a SageMaker endpoint (real-time, real-time scale-to-zero, or async) with autoscaling. Use whenever about to create a SageMaker endpoint, write deployment code that calls create_endpoint |
| [references/hf-cloud-serving-image-selection.md](references/hf-cloud-serving-image-selection.md) | Pick the right serving container for a SageMaker model deployment and find its current. Use whenever about to deploy a model to a SageMaker endpoint and an image URI needs to be chosen — including |

<!-- merge-skill-families:v1 -->
