window.addEventListener("load", function () {
  let blockedDomains = [];
  fetch('https://hubspotonwebflow.com/assets/js/blockedDomains.json')
  .then(response => response.json())
  .then(data => {
    blockedDomains = data;
  })
  .catch(error => console.error('Error:', error));

  const updateFormData = (formData) => {
    for (let [name, value] of formData.entries()) {
      switch (name) {
        case "hutk":
          const cookies = document.cookie.split(";");
          const cookieMap = {};

          cookies.forEach((cookie) => {
            const [name, value] = cookie.trim().split("=");
            cookieMap[name] = value;
          });

          const hubspotCookie = cookieMap["hubspotutk"];
          if (hubspotCookie) {
            formData.set(name, hubspotCookie);
          }
          break;
        case "pageUri":
          formData.set(name, window.location.href);
          break;
        case "pageName":
          formData.set(name, document.title);
          break;
        case "pageId":
          formData.set(name, window.location.pathname);
          break;
        default:
          break;
      }
    }
    return formData;
  }

  const webflowHubSpotForms = document.querySelectorAll("[data-wf-hs-form]");
  if (webflowHubSpotForms.length > 0) {
    webflowHubSpotForms.forEach(async (form) => {
      const actionUrl = new URL(form.action);
      const pathParts = actionUrl.pathname.split('/');
      const id = pathParts[pathParts.length - 1];
      let blockList = false;

      await fetch(`https://hubspotonwebflow.com/api/forms/blockList?id=${id}`)
      .then(response => response.json())
      .then(data => {
        blockList = data;
      })
      .catch(error => console.error('Error:', error));

    let isBlocked = false;
    const submitButton = form.querySelector('input[type="submit"], button[type="submit"]');
    if(blockList && blockList.enabled) {
      let additionalBlockedDomains = [];
      if(blockList.additionalBlockedDomains && Array.isArray(blockList.additionalBlockedDomains) && blockList.additionalBlockedDomains.length > 0) {
        additionalBlockedDomains = blockList.additionalBlockedDomains;
        blockedDomains.push(...additionalBlockedDomains);
      }
      const emailInputs = form.querySelectorAll('input[type="email"]');
      emailInputs.forEach((input) => {
        input.addEventListener('input', () => {
          const email = input.value;
          const emailDomain = email.split('@')[1];

          const warningMessage = document.createElement('p');
          warningMessage.style.color = 'red';
          warningMessage.style.marginTop = '1rem';
          warningMessage.style.marginBottom = '1rem';
          warningMessage.style.fontSize = '1rem';
          warningMessage.style.display = 'none';
          const existingWarningMessage = input.parentNode.querySelector('.warning-message');
          if (blockedDomains.includes(emailDomain)) {
            isBlocked = true;
            submitButton.disabled = true;
            submitButton.style.cursor = 'not-allowed';
            submitButton.style.backgroundColor = 'grey';
            warningMessage.className = 'warning-message';
            warningMessage.textContent = 'This email domain is blocked. Please enter a different email.';
            warningMessage.style.display = 'block';
            if (existingWarningMessage) {
              input.parentNode.removeChild(existingWarningMessage);
            }

            input.parentNode.appendChild(warningMessage);
          } else {
            isBlocked = false;
            submitButton.disabled = false;
            submitButton.style.cursor = '';
            submitButton.style.backgroundColor = '';
            if(existingWarningMessage) {
              input.parentNode.removeChild(existingWarningMessage);
            }
          }
        });
      });
    }

    const checkboxes = form.querySelectorAll('input[type="checkbox"][required]');
      const checkboxMap = {};
      checkboxes.forEach((checkbox) => {
        const name = checkbox.name;

        if (!checkboxMap[name]) {
          checkboxMap[name] = [];
        }

        checkboxMap[name].push(checkbox);
      });

      let isCheckboxValidationFailed = false;
      Object.values(checkboxMap).forEach((checkboxes) => {
        if (checkboxes.length > 1) {
          const errorMessage = document.createElement('div');
          errorMessage.textContent = 'At least one checkbox must be checked.';
          errorMessage.style.color = 'red';
          errorMessage.style.display = 'none';
          errorMessage.style.marginTop = '1rem';
          errorMessage.style.marginBottom = '1rem';
          errorMessage.style.fontSize = '1rem';
          checkboxes.forEach((checkbox) => {
            checkbox.required = false;

            checkbox.addEventListener('change', () => {
              if (checkbox.checked) {
                errorMessage.style.display = 'none';
                isCheckboxValidationFailed = false;
              }
            });
          });

          const form = checkboxes[0].form;
          form.appendChild(errorMessage);

          form.addEventListener('submit', (event) => {
            const isChecked = checkboxes.some((checkbox) => checkbox.checked);
            if (!isChecked) {
              event.preventDefault();
              errorMessage.style.display = 'block';
              isCheckboxValidationFailed = true;
            } else {
              errorMessage.style.display = 'none';
              isCheckboxValidationFailed = false;
            }
          });
        }
      });

      form.addEventListener("submit", (event) => {
        if (isCheckboxValidationFailed) {
          return;
        }

        event.preventDefault();
        let formData = new FormData(form);
        formData = updateFormData(formData);

        fetch(form.action, {
          method: form.method,
          body: formData,
        })
          .then((response) => response.json())
          .then((data) => {
            if ("redirectUri" in data) {
              window.location.href = data.redirectUri;
            }

            if ("inlineMessage" in data) {
              const message = document.createElement("div");
              message.style.marginTop = "1rem";
              message.style.marginBottom = "1rem";
              message.innerHTML = data.inlineMessage;
              form.appendChild(message);
              message.scrollIntoView({ behavior: "smooth", block: "center" });
            }
          })
          .catch((error) => console.error(error));
      });
    });
  }

  const webflowForms = document.querySelectorAll(
    "[data-webflow-hubspot-api-form-url]"
  );
  if (webflowForms.length > 0) {
    webflowForms.forEach(async (form) => {
      const actionUrl = new URL(form.dataset.webflowHubspotApiFormUrl);
      const pathParts = actionUrl.pathname.split('/');
      const id = pathParts[pathParts.length - 1];
      let blockList = false;

      await fetch(`https://hubspotonwebflow.com/api/forms/blockList?id=${id}`)
      .then(response => response.json())
      .then(data => {
        blockList = data;
      })
      .catch(error => console.error('Error:', error));

      let isBlocked = false;
      const submitButton = form.querySelector('input[type="submit"], button[type="submit"]');
      if(blockList && blockList.enabled) {
        let additionalBlockedDomains = [];
        if(blockList.additionalBlockedDomains && Array.isArray(blockList.additionalBlockedDomains) && blockList.additionalBlockedDomains.length > 0) {
          additionalBlockedDomains = blockList.additionalBlockedDomains;
          blockedDomains.push(...additionalBlockedDomains);
        }
        const emailInputs = form.querySelectorAll('input[type="email"]');
        emailInputs.forEach((input) => {
          input.addEventListener('input', () => {
            const email = input.value;
            const emailDomain = email.split('@')[1];

            const warningMessage = document.createElement('p');
            warningMessage.style.color = 'red';
            warningMessage.style.marginTop = '1rem';
            warningMessage.style.marginBottom = '1rem';
            warningMessage.style.fontSize = '1rem';
            warningMessage.style.display = 'none';
            const existingWarningMessage = input.parentNode.querySelector('.warning-message');
            if (blockedDomains.includes(emailDomain)) {
              isBlocked = true;
              submitButton.disabled = true;
              submitButton.style.cursor = 'not-allowed';
              submitButton.style.backgroundColor = 'grey';
              warningMessage.className = 'warning-message';
              warningMessage.textContent = 'This email domain is blocked. Please enter a different email.';
              warningMessage.style.display = 'block';
              if (existingWarningMessage) {
                input.parentNode.removeChild(existingWarningMessage);
              }

              input.parentNode.appendChild(warningMessage);
            } else {
              isBlocked = false;
              submitButton.disabled = false;
              submitButton.style.cursor = '';
              submitButton.style.backgroundColor = '';
              if(existingWarningMessage) {
                input.parentNode.removeChild(existingWarningMessage);
              }
            }
          });
        });
      }

      form.addEventListener("submit", (event) => {
        event.preventDefault();
        let formData = new FormData(form);
        formData = updateFormData(formData);
        form.querySelectorAll("[data-wfhsfieldname]").forEach((field) => {
          if (field.type === "file") {
            formData.set(field.dataset.wfhsfieldname, field.files[0]);
          } else if(field.type === 'checkbox') {
            if(field.checked) {
              formData.set(field.dataset.wfhsfieldname, field.value);
            }
          } else if(field.type === 'radio') {
            if(field.checked) {
              formData.set(field.dataset.wfhsfieldname, field.value);
            }
          } else {
            formData.set(field.dataset.wfhsfieldname, field.value);
          }
          // formData.delete(field.name);
        });

        fetch(form.dataset.webflowHubspotApiFormUrl, {
          method: "POST",
          body: formData,
        })
          .then((response) => response.json())
          .catch((error) => console.error(error));
      });
    });
  }
});
