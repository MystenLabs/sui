/* global window, $ */

/// Demo Front-End functionality
export const closeMobleMenu = () => {
  $('.header .js-burger').trigger('click')
}

export const scrollToElment = (refName) => {
  const element = window.document.querySelector(`#${refName}`)
  if (!element) return
  window.scrollTo({ top: element.offsetTop, behavior: 'smooth' })
}

/// update hash in url and active menu item
export const scrollActiveMenu = () => {
  const scrollPos = window.pageYOffset || document.documentElement.scrollTop

  /// list all table of content menu items
  $('._t-content li a').each(function () {
    const currLink = $(this)

    /// get it
    const b = currLink.attr('href').split('#')[1]

    const refElement2 = $('#' + b)
    if (
      refElement2.position().top <= scrollPos &&
      refElement2.position().top + refElement2.height() > scrollPos
    ) {
      $('._t-content li').removeClass('active')
      currLink.parent().addClass('active')
      $('._t-content li a').removeClass('_selectActive')
      // window.location.hash = '#' + b;
    } else {
      currLink.removeClass('active')
    }
  })
}

export const baseFn = () => {
  'use strict'

  // $(document).on("scroll", scrollActiveMenu);
  /* * ==========================================================================
       * ==========================================================================
       * ==========================================================================
       *
       *
       * [Table of Contents]
       *
       * 1. animations
       * 2. burger
       * 3. button
       * 4. circle
       * 5. figureFeature
       * 6. figurePortfolio
       * 7. figurePost
       * 8. figureService
       * 9. form
       * 10. gmap
       * 11. grid
       * 12. header
       * 13. menu
       * 14. preloader
       * 15. sectionCTA
       * 16. sectionFeatures
       * 17. sectionFullscreen
       * 18. sectionHeader
       * 19. sectionInfo
       * 20. sectionIntro
       * 21. sectionLogos
       * 22. sectionMasthead
       * 23. sectionSteps
       * 24. slider
       * 25. sliderFullscreen
       * 26. sliderFullscreen4
       * 27. sliderPortfolioItem
       * 28. sliderServices
       * 29. sliderTestimonials
       * 30. social
       * 31. splitText

       * ==========================================================================
       * ==========================================================================
       * ==========================================================================
       */

  'use strict'

  window.SMController = new ScrollMagic.Controller()
  window.SMSceneTriggerHook = 0.85
  window.SMSceneReverse = false

  $(document).ready(function () {
    const sectionFullscreen4 = new SectionFullscreen4()
    const sectionFullscreen1 = new SectionFullscreen1()
    const figurePortfolio = new FigurePortfolio()
    const sectionHeader = new SectionHeader()
    const sectionIntro = new SectionIntro()
    const sliderServices = new SliderServices()
    const sectionInfo = new SectionInfo()
    const sectionCTA = new SectionCTA()
    const figurePost = new FigurePost()
    const sliderTestimonials = new SliderTestimonials()
    const sectionLogos = new SectionLogos()
    const sectionFeatures = new SectionFeatures()
    const sectionSteps = new SectionSteps()
    const sectionMasthead = new SectionMasthead()

    // $('.jarallax').jarallax({
    // 	speed: 1.2,
    // 	imgSize: 'contain'
    // });

    objectFitImages()

    new SliderPortfolioItem()
    new SliderFullscreen1()
    new SliderFullscreen4()
    new FigureFeature()
    new FigureService()
    new GMap()
    new Form()
    new Burger()
    new Social()
    new Button()
    new Menu()

    new Preloader(function () {
      new Grid()
      sectionFullscreen4.animate()
      sectionFullscreen1.animate()
      sectionHeader.animate()
      sectionIntro.animate()
      figurePortfolio.animate()
      sliderServices.animate()
      sectionInfo.animate()
      sectionCTA.animate()
      figurePost.animate()
      sliderTestimonials.animate()
      sectionLogos.animate()
      sectionFeatures.animate()
      sectionSteps.animate()
      sectionMasthead.animate()
    })
  })

  /* ======================================================================== */
  /* 1. animations */
  /* ======================================================================== */
  function createOSScene($el, tl) {
    new $.ScrollMagic.Scene({
      triggerElement: $el,
      triggerHook: SMSceneTriggerHook,
      reverse: SMSceneReverse,
    })
      .setTween(tl)
      .addTo(SMController)
  }

  function animateCurtainImg($curtain, $img) {
    const tl = new TimelineMax()

    return tl
      .to($curtain, 0.3, {
        x: '0%',
        ease: Expo.easeInOut,
      })
      .to($curtain, 0.4, {
        y: '0%',
        ease: Expo.easeInOut,
      })
      .set($img, {
        autoAlpha: 1,
      })
      .to($img, 1, {
        scale: 1,
        ease: Power4.easeOut,
      })
      .to(
        $curtain,
        0.3,
        {
          y: '102%',
          ease: Expo.easeInOut,
        },
        '-=1'
      )
  }

  function animateCurtainContent($curtain, $content) {
    const tl = new TimelineMax()

    return tl
      .to($curtain, 0.3, {
        x: '0%',
        ease: Expo.easeInOut,
      })
      .to($curtain, 0.4, {
        y: '0%',
        ease: Expo.easeInOut,
      })
      .set($content, {
        autoAlpha: 1,
      })
      .to($curtain, 0.3, {
        y: '102%',
        ease: Expo.easeInOut,
      })
  }

  function setCurtainImg($curtain, $img) {
    TweenMax.set($img, {
      scale: 1.1,
      autoAlpha: 0,
    })

    TweenMax.set($curtain, {
      y: '-99%',
      x: '-100%',
    })
  }

  function setCurtainContent($curtain, $content) {
    TweenMax.set($content, {
      autoAlpha: 0,
    })

    TweenMax.set($curtain, {
      y: '-99%',
      x: '-100%',
    })
  }

  /* ======================================================================== */
  /* 2. burger */
  /* ======================================================================== */
  window.closeMenuFn = function () {
    const header = new Header()
    const $burger = $('.js-burger')

    if ($burger.hasClass('.js-burger')) {
      $burger.removeClass('burger_opened')
      header.closeOverlayMenu()
    }
  }

  const Burger = function () {
    const $burger = $('.js-burger')

    const OPEN_CLASS = 'burger_opened'

    const header = new Header()
    $burger.on('click', function (e) {
      e.preventDefault()

      if (!e.detail || e.detail == 1) {
        //  var $burger = $(this);

        if ($burger.hasClass(OPEN_CLASS)) {
          $burger.removeClass(OPEN_CLASS)
          header.closeOverlayMenu()
        } else {
          $burger.addClass(OPEN_CLASS)
          header.openOverlayMenu()
        }
      }
    })
  }

  /* ======================================================================== */
  /* 3. button */
  /* ======================================================================== */
  var Button = function () {
    $('.button-square').each(function () {
      const $el = $(this)
      const $rect = $el.find('.rect')

      TweenMax.set($rect, {
        drawSVG: 0,
        stroke: '#b68c70',
      })

      $el
        .on('mouseenter touchstart', function () {
          TweenMax.to($rect, 0.6, {
            drawSVG: true,
            ease: Power4.easeInOut,
          })
        })
        .on('mouseleave touchend', function () {
          TweenMax.to($rect, 0.6, {
            drawSVG: false,
            ease: Power4.easeInOut,
          })
        })
    })
  }

  /* ======================================================================== */
  /* 4. circle */
  /* ======================================================================== */
  const Circle = function () {
    this.animate = function ($el) {
      const $circle = $el.find('.circle')

      if (!$circle.length) {
        return
      }

      TweenMax.set($circle, {
        drawSVG: 0,
        stroke: '#b68c70',
      })

      $el
        .on('mouseenter touchstart', function () {
          TweenMax.to($circle, 0.6, {
            drawSVG: true,
            ease: Power4.easeInOut,
          })
        })
        .on('mouseleave touchend', function () {
          TweenMax.to($circle, 0.6, {
            drawSVG: false,
            ease: Power4.easeInOut,
          })
        })
    }
  }

  /* ======================================================================== */
  /* 5. figureFeature */
  /* ======================================================================== */
  var FigureFeature = function () {
    const $elements = $('.figure-feature')

    if (!$elements.length) {
      return
    }

    const circle = new Circle()

    $elements.each(function () {
      circle.animate($(this))
    })
  }

  /* ======================================================================== */
  /* 6. figurePortfolio */
  /* ======================================================================== */
  var FigurePortfolio = function () {
    const $target = $('.figure-portfolio[data-os-animation]')
    const $img = $target.find('.overflow__content')
    const $curtain = $target.find('.overflow__curtain')
    const $heading = $target.find('.figure-portfolio__header h2')
    const $headline = $target.find('.figure-portfolio__headline')
    const $info = $target.find('.figure-portfolio__info')
    const splitHeading = splitLines($heading)
    const splitInfo = splitLines($info)

    prepare()

    function prepare() {
      if (!$target.length) {
        return
      }

      setLines(splitHeading.words)
      setLines(splitInfo.words)

      TweenMax.set($headline, {
        scaleX: 0,
        transformOrigin: 'left center',
      })

      setCurtainImg($curtain, $img)
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }

      $target.each(function () {
        const $el = $(this)
        const tl = new TimelineMax()
        const $elLink = $el.find('.figure-portfolio__link')
        const $elImg = $el.find($img)
        const $elCurtain = $el.find($curtain)
        const $elHeading = $el.find($heading)
        const $elHeadline = $el.find($headline)
        const elSplitInfo = $el.find(splitInfo.words)
        const elSplitHeading = $el.find(splitHeading.words)

        tl.add(animateCurtainImg($elCurtain, $elImg))
          .to(
            $elHeadline,
            0.6,
            {
              scaleX: 1,
              ease: Power4.easeOut,
            },
            '-=1'
          )
          .add(animateLines(elSplitInfo), '-=0.8')
          .add(animateLines(elSplitHeading), '-=0.8')

        createOSScene($el, tl)

        $elLink
          .on('mouseenter touchstart', function () {
            TweenMax.to($elImg, 0.3, {
              scale: 1.1,
              ease: Power3.easeInOut,
            })

            TweenMax.to($elHeadline, 0.3, {
              scaleX: 0.8,
              ease: Power3.easeInOut,
              transformOrigin: 'right center',
            })

            TweenMax.to($elHeading, 0.3, {
              x: '10px',
              ease: Power3.easeInOut,
            })
          })
          .on('mouseleave touchend', function () {
            TweenMax.to($elImg, 0.3, {
              scale: 1,
              ease: Power2.easeInOut,
            })

            TweenMax.to($elHeadline, 0.3, {
              scaleX: 1,
              ease: Power2.easeInOut,
              transformOrigin: 'right center',
            })

            TweenMax.to($elHeading, 0.3, {
              x: '0px',
              ease: Power2.easeInOut,
            })
          })
      })
    }
  }

  /* ======================================================================== */
  /* 7. figurePost */
  /* ======================================================================== */
  var FigurePost = function () {
    const $target = $('.figure-post[data-os-animation]')
    const $heading = $target.find('.figure-post__content h3')
    const $text = $target.find('.figure-post__content p')
    const splitHeading = splitLines($heading)
    const splitDescr = splitLines($text)

    prepare()

    function prepare() {
      if (!$target.length) {
        return
      }

      setLines(splitHeading.words)
      if (splitDescr) {
        setLines(splitDescr.lines)
      }
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }

      $target.each(function () {
        const $el = $(this)
        const tl = new TimelineMax()
        const $elHeading = $el.find($heading)
        const elSplitDescr = $elHeading.find(splitDescr.lines)
        const elSplitHeading = $elHeading.find(splitHeading.words)

        tl.add(animateLines(elSplitHeading))
        if (splitDescr) {
          tl.add(animateLines(elSplitDescr, 1, 0.1))
        }

        createOSScene($el, tl)
      })
    }
  }

  /* ======================================================================== */
  /* 8. figureService */
  /* ======================================================================== */
  var FigureService = function () {
    const $target = $('.figure-service')

    if (!$target.length) {
      return
    }

    const circle = new Circle()
    const $icons = $target.find('.figure-service__icon')
    const $headlines = $target.find('.figure-service__headline')
    const $numbers = $target.find('.figure-service__number')
    const $texts = $target.find('.figure-service__header p')
    const splitDescr = new SplitText($texts, {
      type: 'lines',
      linesClass: 'split-line',
    })

    setLines(splitDescr.lines)

    $target.each(function () {
      const $el = $(this)
      const $elIcon = $el.find($icons)
      const $elHeadline = $el.find($headlines)
      const $elNumber = $el.find($numbers)
      const tl = new TimelineMax()
      const elDescr = $el.find(splitDescr.lines)

      circle.animate($el)

      $el
        .on('mouseenter touchstart', function () {
          tl.clear()
            .to($elHeadline, 0.6, {
              scaleX: 2,
              ease: Power4.easeOut,
            })
            .to(
              $elNumber,
              0.3,
              {
                y: '-50px',
                autoAlpha: 0,
              },
              '-=0.6'
            )
            .to(
              $elIcon,
              0.6,
              {
                y: '-50px',
                ease: Power4.easeOut,
              },
              '-=0.6'
            )
            .add(animateLines(elDescr, 0.6, 0.1), '-=0.6')
        })
        .on('mouseleave touchend', function () {
          tl.clear()
            .to($elHeadline, 0.3, {
              scaleX: 1,
            })
            .to(
              $elNumber,
              0.3,
              {
                y: '0px',
                autoAlpha: 1,
              },
              '-=0.3'
            )
            .to(
              $elIcon,
              0.3,
              {
                y: '0px',
              },
              '-=0.3'
            )
            .to(
              elDescr,
              0.3,
              {
                y: '100%',
                autoAlpha: 0,
              },
              '-=0.3'
            )
        })
    })
  }

  /* ======================================================================== */
  /* 9. form */
  /* ======================================================================== */
  var Form = function () {
    floatLabels()
    ajaxForm()

    function floatLabels() {
      const INPUT_CLASS = '.input-float__input'
      const INPUT_NOT_EMPTY = 'input-float__input_not-empty'
      const INPUT_FOCUSED = 'input-float__input_focused'

      if (!$(INPUT_CLASS).length) {
        return
      }

      $(INPUT_CLASS).each(function () {
        const $currentField = $(this)

        if ($currentField.val()) {
          $currentField.addClass(INPUT_NOT_EMPTY)
        } else {
          $currentField.removeClass([INPUT_FOCUSED, INPUT_NOT_EMPTY])
        }
      })

      $(document)
        .on('focusin', INPUT_CLASS, function () {
          const $currentField = $(this)

          $currentField.addClass(INPUT_FOCUSED).removeClass(INPUT_NOT_EMPTY)
        })
        .on('focusout', INPUT_CLASS, function () {
          const $currentField = $(this)

          // delay needed due to issues with datepicker
          if ($currentField.val()) {
            $currentField.removeClass(INPUT_FOCUSED).addClass(INPUT_NOT_EMPTY)
          } else {
            $currentField.removeClass(INPUT_FOCUSED)
          }
        })
    }

    function ajaxForm() {
      const $form = $('.js-ajax-form')

      if (!$form.length) {
        return
      }

      $form.validate({
        errorElement: 'span',
        errorPlacement(error, element) {
          error.appendTo(element.parent()).addClass('form__error')
        },
        submitHandler(form) {
          ajaxSubmit(form)
        },
      })

      function ajaxSubmit(form) {
        $.ajax({
          type: $form.attr('method'),
          url: $form.attr('action'),
          data: $form.serialize(),
        })
          .done(function () {
            alert($form.attr('data-message-success'))
            $form.trigger('reset')
            floatLabels()
          })
          .fail(function () {
            alert($form.attr('data-message-error'))
          })
      }
    }
  }

  /* ======================================================================== */
  /* 10. gmap */
  /* ======================================================================== */
  var GMap = function () {
    const $mapContainer = $('#js-gmap')

    if ($mapContainer.length) {
      const LAT = parseFloat($mapContainer.attr('data-gmap-lat'))
      const LON = parseFloat($mapContainer.attr('data-gmap-lon'))
      const ZOOM = parseInt($mapContainer.attr('data-gmap-zoom'))
      const MARKER = $mapContainer.attr('data-gmap-marker')
      const TITLE = $mapContainer.attr('data-gmap-title')

      const map = new google.maps.Map(
        document.getElementById($mapContainer[0].id),
        {
          center: {
            lat: LAT,
            lng: LON,
          },
          zoom: ZOOM,
          styles: [
            {
              featureType: 'water',
              elementType: 'geometry',
              stylers: [
                {
                  color: '#e9e9e9',
                },
                {
                  lightness: 17,
                },
              ],
            },
            {
              featureType: 'landscape',
              elementType: 'geometry',
              stylers: [
                {
                  color: '#f5f5f5',
                },
                {
                  lightness: 20,
                },
              ],
            },
            {
              featureType: 'road.highway',
              elementType: 'geometry.fill',
              stylers: [
                {
                  color: '#ffffff',
                },
                {
                  lightness: 17,
                },
              ],
            },
            {
              featureType: 'road.highway',
              elementType: 'geometry.stroke',
              stylers: [
                {
                  color: '#ffffff',
                },
                {
                  lightness: 29,
                },
                {
                  weight: 0.2,
                },
              ],
            },
            {
              featureType: 'road.arterial',
              elementType: 'geometry',
              stylers: [
                {
                  color: '#ffffff',
                },
                {
                  lightness: 18,
                },
              ],
            },
            {
              featureType: 'road.local',
              elementType: 'geometry',
              stylers: [
                {
                  color: '#ffffff',
                },
                {
                  lightness: 16,
                },
              ],
            },
            {
              featureType: 'poi',
              elementType: 'geometry',
              stylers: [
                {
                  color: '#f5f5f5',
                },
                {
                  lightness: 21,
                },
              ],
            },
            {
              featureType: 'poi.park',
              elementType: 'geometry',
              stylers: [
                {
                  color: '#dedede',
                },
                {
                  lightness: 21,
                },
              ],
            },
            {
              elementType: 'labels.text.stroke',
              stylers: [
                {
                  visibility: 'on',
                },
                {
                  color: '#ffffff',
                },
                {
                  lightness: 16,
                },
              ],
            },
            {
              elementType: 'labels.text.fill',
              stylers: [
                {
                  saturation: 36,
                },
                {
                  color: '#333333',
                },
                {
                  lightness: 40,
                },
              ],
            },
            {
              elementType: 'labels.icon',
              stylers: [
                {
                  visibility: 'off',
                },
              ],
            },
            {
              featureType: 'transit',
              elementType: 'geometry',
              stylers: [
                {
                  color: '#f2f2f2',
                },
                {
                  lightness: 19,
                },
              ],
            },
            {
              featureType: 'administrative',
              elementType: 'geometry.fill',
              stylers: [
                {
                  color: '#fefefe',
                },
                {
                  lightness: 20,
                },
              ],
            },
            {
              featureType: 'administrative',
              elementType: 'geometry.stroke',
              stylers: [
                {
                  color: '#fefefe',
                },
                {
                  lightness: 17,
                },
                {
                  weight: 1.2,
                },
              ],
            },
          ],
        }
      )

      const marker = new google.maps.Marker({
        position: new google.maps.LatLng(LAT, LON),
        map,
        // title: TITLE,
        icon: MARKER,
      })

      marker.addListener('click', function () {
        const infowindow = new google.maps.InfoWindow({
          content: TITLE,
        })
        infowindow.open(map, marker)
      })
    }
  }

  /* ======================================================================== */
  /* 11. grid */
  /* ======================================================================== */
  var Grid = function () {
    const $grid = $('.js-grid')

    if (!$grid.length) {
      return
    }

    $grid.masonry({
      itemSelector: '.js-grid__item',
      columnWidth: '.js-grid__sizer',
      horizontalOrder: true,
    })
  }

  /* ======================================================================== */
  /* 12. header */
  /* ======================================================================== */
  var Header = function () {
    const $overlay = $('.header__wrapper-overlay-menu')
    const $menuLinks = $('.overlay-menu > li > a .overlay-menu__item-wrapper')
    const $submenu = $('.overlay-sub-menu')
    const $submenuButton = $('.js-submenu-back')
    const $submenuLinks = $submenu.find('> li > a .overlay-menu__item-wrapper')

    setOverlayMenu()
    stickHeader()

    function setOverlayMenu() {
      TweenMax.set([$overlay, $menuLinks, $submenuLinks], {
        y: '100%',
      })

      TweenMax.set([$submenu, $submenuButton], {
        autoAlpha: 0,
        y: '10px',
      })
    }

    this.closeOverlayMenu = function () {
      const tl = new TimelineMax()

      tl.timeScale(2)

      tl.to([$menuLinks, $submenuLinks], 0.6, {
        y: '-100%',
        ease: Power4.easeIn,
      })
        .to($submenuButton, 0.6, {
          y: '-10px',
          autoAlpha: 0,
        })
        .to($overlay, 1, {
          y: '-100%',
          ease: Expo.easeInOut,
        })
        .add(function () {
          setOverlayMenu()
        })
    }

    this.openOverlayMenu = function () {
      const tl = new TimelineMax()

      tl.to($overlay, 1, {
        y: '0%',
        ease: Expo.easeInOut,
      }).staggerTo(
        $menuLinks,
        0.6,
        {
          y: '0%',
          ease: Power4.easeOut,
        },
        0.05,
        '-=0.3'
      )
    }

    function stickHeader() {
      const $header = $('.js-header-sticky')

      new $.ScrollMagic.Scene({
        offset: '1px',
      })
        .setPin($header, {
          pushFollowers: false,
        })
        .setClassToggle($header, 'header_sticky')
        .addTo(SMController)
    }
  }

  /* ======================================================================== */
  /* 13. menu */
  /* ======================================================================== */
  var Menu = function () {
    const $menu = $('.js-overlay-menu')

    if (!$menu.length) {
      return
    }

    const $links = $menu.find('.menu-item-has-children > a')
    const $submenus = $menu.find('.overlay-sub-menu')
    const $submenuButton = $('.js-submenu-back')
    const OPEN_CLASS = 'opened'
    const tl = new TimelineMax()

    function openSubmenu($submenu, $currentMenu) {
      const $currentLinks = $currentMenu.find(
        '> li > a .overlay-menu__item-wrapper'
      )
      const $submenuLinks = $submenu.find(
        '> li > a .overlay-menu__item-wrapper'
      )

      tl.stop()
        .play()
        .set($submenu, {
          autoAlpha: 1,
          zIndex: 100,
        })
        .to(
          $currentLinks,
          0.6,
          {
            y: '-100%',
            ease: Power4.easeIn,
          },
          '-=0.3'
        )
        .staggerTo(
          $submenuLinks,
          0.6,
          {
            y: '0%',
            ease: Power4.easeOut,
          },
          0.05
        )

      $submenus.removeClass(OPEN_CLASS)
      $submenu.not($menu).addClass(OPEN_CLASS)

      if ($submenus.hasClass(OPEN_CLASS)) {
        tl.to(
          $submenuButton,
          0.3,
          {
            autoAlpha: 1,
            y: '0px',
          },
          '-=0.6'
        )
      } else {
        tl.to(
          $submenuButton,
          0.3,
          {
            autoAlpha: 0,
            y: '10px',
          },
          '-=0.6'
        )
      }
    }

    function closeSubmenu($submenu, $currentMenu) {
      const $currentLinks = $currentMenu.find(
        '> li > a .overlay-menu__item-wrapper'
      )
      const $submenuLinks = $submenu.find(
        '> li > a .overlay-menu__item-wrapper'
      )

      tl.stop()
        .play()
        .set($submenu, {
          zIndex: -1,
        })
        .to(
          $submenuLinks,
          0.6,
          {
            y: '100%',
            ease: Power4.easeIn,
          },
          '-=0.3'
        )
        .staggerTo(
          $currentLinks,
          0.6,
          {
            y: '0%',
            ease: Power4.easeOut,
          },
          0.05
        )
        .set($submenu, {
          autoAlpha: 0,
          y: '10px',
        })

      $submenus.removeClass(OPEN_CLASS)
      $currentMenu.not($menu).addClass(OPEN_CLASS)

      if ($submenus.hasClass(OPEN_CLASS)) {
        TweenMax.to(
          $submenuButton,
          0.3,
          {
            autoAlpha: 1,
            y: '0px',
          },
          '-=0.6'
        )
      } else {
        TweenMax.to(
          $submenuButton,
          0.3,
          {
            autoAlpha: 0,
            y: '10px',
          },
          '-=0.6'
        )
      }
    }

    $links.on('click', function (e) {
      e.preventDefault()

      if (!e.detail || e.detail == 1) {
        const $el = $(this)
        const $currentMenu = $el.parents('ul')
        const $submenu = $el.next('.overlay-sub-menu')

        openSubmenu($submenu, $currentMenu)
      }
    })

    $submenuButton.on('click', function (e) {
      e.preventDefault()

      if (!e.detail || e.detail == 1) {
        const $el = $(this)
        const $openedMenu = $submenus.filter('.' + OPEN_CLASS)
        const $prevMenu = $openedMenu.parent('li').parent('ul')

        closeSubmenu($openedMenu, $prevMenu)
      }
    })
  }

  /* ======================================================================== */
  /* 14. preloader */
  /* ======================================================================== */
  var Preloader = function (callback) {
    const $preloader = $('.preloader')
    const $curtain = $preloader.find('.preloader__curtain')
    const $logo = $preloader.find('.preloader__logo')
    const $rect = $logo.find('.rect')
    const tl = new TimelineMax()

    load()

    $('body')
      .imagesLoaded()
      .always(
        {
          background: true,
        },
        function () {
          if (!$preloader.length) {
            callback()
            return
          }

          finish()
        }
      )

    function finish() {
      tl.clear()
        .to($rect, 2, {
          drawSVG: true,
          ease: Expo.easeInOut,
        })
        .to(
          $logo,
          0.3,
          {
            autoAlpha: 0,
          },
          '-=0.3'
        )
        .staggerTo(
          $curtain,
          1,
          {
            y: '-100%',
            ease: Expo.easeInOut,
          },
          0.05,
          '-=0.3'
        )
        .set($preloader, {
          autoAlpha: 0,
        })
        .add(function () {
          callback()
        }, '-=0.4')
    }

    function load() {
      tl.fromTo(
        $rect,
        15,
        {
          drawSVG: 0,
          stroke: '#b68c70',
          ease: SlowMo.ease.config(0.7, 0.7, false),
        },
        {
          drawSVG: true,
        }
      )
    }

    this.curtainsUp = function () {
      tl.staggerTo(
        $curtain,
        1,
        {
          y: '-100%',
          ease: Expo.easeInOut,
        },
        0.05
      ).set($preloader, {
        autoAlpha: 0,
      })
    }

    this.curtainsDown = function () {
      tl.set($preloader, {
        autoAlpha: 1,
      })
        .staggerTo(
          $curtain,
          1,
          {
            y: '0%',
            ease: Expo.easeInOut,
          },
          0.05
        )
        .set($rect, {
          drawSVG: 0,
        })
        .to($logo, 0.6, {
          autoAlpha: 1,
        })
    }
  }

  /* ======================================================================== */
  /* 15. sectionCTA */
  /* ======================================================================== */
  var SectionCTA = function () {
    const $target = $('.section-cta[data-os-animation]')
    const $header = $target.find('.section-cta__header')
    const $headline = $target.find('.section-cta__headline')
    const $heading = $header.find('h2, h4')
    const $button = $target.find('.section-cta__wrapper-button')
    const splitHeading = splitLines($heading)

    prepare()

    function prepare() {
      if (!$target.length) {
        return
      }

      setLines(splitHeading.lines)

      TweenMax.set($button, {
        autoAlpha: 0,
        y: '30px',
      })

      TweenMax.set($headline, {
        scaleX: 0,
        transformOrigin: 'left center',
      })
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }

      $target.each(function () {
        const $el = $(this)
        const elLines = $el.find(splitHeading.lines)
        const $elHeader = $el.find($header)
        const tl = new TimelineMax()

        tl.add(animateLines(elLines, 1, 0.1))
          .to(
            $button,
            0.6,
            {
              autoAlpha: 1,
              y: '0px',
            },
            '-=0.8'
          )
          .to(
            $headline,
            0.6,
            {
              scaleX: 1,
              ease: Expo.easeInOut,
            },
            '-=0.6'
          )

        createOSScene($elHeader, tl)
      })
    }
  }

  /* ======================================================================== */
  /* 16. sectionFeatures */
  /* ======================================================================== */
  var SectionFeatures = function () {
    const $target = $('.section-features[data-os-animation]')
    const $heading = $('.figure-feature__header h3')
    const $text = $('.figure-feature__header p')
    const $icon = $('.figure-feature__icon')
    const splitDescr = splitLines($text)
    const splitHeading = splitLines($heading)
    const tl = new TimelineMax()

    prepare()

    function prepare() {
      if (!$target.length) {
        return
      }

      setLines(splitHeading.words)
      setLines(splitDescr.lines)

      TweenMax.set($icon, {
        autoAlpha: 0,
        y: '30px',
      })
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }
      tl.staggerTo(
        $icon,
        0.6,
        {
          autoAlpha: 1,
          y: '0px',
          ease: Power4.easeOut,
        },
        0.05
      )
      tl.add(animateLines(splitHeading.words), '-=0.6')
      tl.add(animateLines(splitDescr.lines), '-=0.6')

      createOSScene($target, tl)
    }
  }

  /* ======================================================================== */
  /* 17. sectionHeader */
  /* ======================================================================== */
  var SectionHeader = function () {
    const $target = $('.section-header[data-os-animation]')
    const $square = $target.find('.section-header__square')
    const $label = $target.find('.section-header__label span')
    const $heading = $target.find('h2')
    const splitHeading = splitLines($heading)
    const splitLabel = splitLines($label)

    prepare()

    function prepare() {
      if (!$target.length) {
        return
      }

      setLines([splitHeading.lines, splitLabel.lines])

      TweenMax.set($square, {
        transformOrigin: 'left center',
        scaleX: 0,
      })
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }

      $target.each(function () {
        const $el = $(this)
        const tl = new TimelineMax()
        const $elSquare = $el.find($square)
        const $elLabel = $el.find($label)
        const $elHeading = $el.find($heading)
        const elSplitHeading = $el.find(splitHeading.lines)
        const elSplitLabel = $el.find(splitLabel.lines)

        tl.to($elSquare, 0.6, {
          scaleX: 1,
          ease: Power4.easeOut,
        })
          .add(animateLines(elSplitLabel, 1, 0.1), '-=1')
          .add(animateLines(elSplitHeading, 1, 0.1), '-=0.8')

        createOSScene($el, tl)
      })
    }
  }

  /* ======================================================================== */
  /* 18. sectionFullscreen */
  /* ======================================================================== */
  var SectionFullscreen4 = function () {
    const $target = $('.section-fullscreen_4[data-os-animation]')
    const tl = new TimelineMax()
    const $headline = $target.find('.slider-fullscreen4__slide-headline')
    const $heading = $target.find('.slider-fullscreen4__slide-header h2')
    const splitHeading = splitLines($heading)

    prepare()

    function prepare() {
      if (!$target.length) {
        return
      }

      setLines(splitHeading.words)

      TweenMax.set($headline, {
        scaleX: 0,
        transformOrigin: 'center center',
      })
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }

      tl.staggerTo(
        $headline,
        0.6,
        {
          scaleX: 1,
          ease: Expo.easeInOut,
        },
        0.05
      ).add(animateLines(splitHeading.words), '-=0.6')
    }
  }

  /* ======================================================================== */
  /* sectionFullscreen1 */
  /* ======================================================================== */
  var SectionFullscreen1 = function () {
    const $target = $('.section-fullscreen_1[data-os-animation]')
    const tl = new TimelineMax()
    const $bg = $target.find('.section-fullscreen__inner-bg')
    const $headline = $target.find('.slider-fullscreen__slide-headline')
    const $heading = $target.find('.slider-fullscreen__slide-header h2')
    const $description = $target.find('.slider-fullscreen__slide-header p')
    const $button = $target.find('.slider-fullscreen__slide-wrapper-button')
    const $img = $target.find('.overflow__content')
    const $curtain = $target.find('.overflow__curtain')
    const splitHeading = splitLines($heading)
    const splitDescription = splitLines($description)

    prepare()

    function prepare() {
      if (!$target.length) {
        return
      }

      setLines(splitHeading.words)
      setLines(splitDescription.lines)

      TweenMax.set($headline, {
        scaleX: 0,
        transformOrigin: 'left center',
      })

      TweenMax.set($bg, {
        scaleY: 0,
        transformOrigin: 'bottom center',
      })

      TweenMax.set($img, {
        scale: 1.1,
        autoAlpha: 0,
      })

      TweenMax.set($button, {
        y: '10px',
        autoAlpha: 0,
      })

      TweenMax.set($curtain, {
        y: '100%',
      })
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }

      tl.staggerTo(
        $bg,
        0.6,
        {
          scaleY: 1,
          ease: Expo.easeInOut,
        },
        0.05
      )
        .to(
          $curtain,
          0.3,
          {
            y: '0%',
            ease: Expo.easeInOut,
          },
          '-=0.6'
        )
        .set($img, {
          autoAlpha: 1,
        })
        .to($img, 0.6, {
          scale: 1,
          ease: Power4.easeOut,
        })
        .to(
          $curtain,
          0.3,
          {
            y: '-102%',
            ease: Expo.easeInOut,
          },
          '-=0.6'
        )
        .to(
          $headline,
          0.6,
          {
            scaleX: 1,
            ease: Expo.easeInOut,
          },
          '-=1'
        )
        .add(animateLines(splitHeading.words), '-=0.6')
        .to(
          $button,
          0.6,
          {
            autoAlpha: 1,
            y: '0px',
          },
          '-=0.6'
        )
        .add(animateLines(splitDescription.lines, 1, 0.1), '-=0.6')
    }
  }

  /* ======================================================================== */
  /* 19. sectionInfo */
  /* ======================================================================== */
  var SectionInfo = function () {
    const $target = $('.section-info[data-os-animation]')
    const $heading = $target.find('.section-info__quote h2')
    const splitHeading = splitLines($heading)

    prepare()

    function prepare() {
      if (!$target.length) {
        return
      }

      setLines(splitHeading.lines)
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }

      $target.each(function () {
        const $el = $(this)
        const elLines = $el.find(splitHeading.lines)
        const tl = new TimelineMax()

        tl.add(animateLines(elLines, 1, 0.1))

        createOSScene($el, tl)
      })
    }
  }

  /* ======================================================================== */
  /* 20. sectionIntro */
  /* ======================================================================== */
  var SectionIntro = function () {
    const $target = $('.section-intro[data-os-animation]')
    const tl = new TimelineMax()
    const $heading = $target.find('h1')
    const $highlight = $heading.find('.highlight__bg')
    const splitHeading = splitLines($heading)

    prepare()

    function prepare() {
      if (!$target.length) {
        return
      }

      setLines(splitHeading.words)

      TweenMax.set($target, {
        scaleY: 0,
        transformOrigin: 'bottom center',
      })

      TweenMax.set($highlight, {
        x: '-100%',
        y: '98%',
      })
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }

      tl.to($target, 1, {
        scaleY: 1,
        ease: Expo.easeInOut,
      })
        .add(animateLines(splitHeading.words), '-=0.4')
        .to(
          $highlight,
          0.6,
          {
            x: '0%',
            ease: Expo.easeInOut,
          },
          '-=0.4'
        )
        .to($highlight, 0.6, {
          y: '0%',
          ease: Expo.easeInOut,
        })

      // createOSScene($target, tl);
    }
  }

  /* ======================================================================== */
  /* 21. sectionLogos */
  /* ======================================================================== */
  var SectionLogos = function () {
    const $target = $(
      '.section-logos[data-os-animation] .section-logos__wrapper-items'
    )
    const tl = new TimelineMax()
    const $logos = $target.find('.section-logos__item')

    prepare()

    function prepare() {
      if (!$target.length) {
        return
      }

      TweenMax.set($logos, {
        y: '30px',
        autoAlpha: 0,
      })
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }

      tl.staggerTo(
        $logos,
        1,
        {
          autoAlpha: 1,
          y: '0px',
          ease: Power4.easeOut,
        },
        0.1
      )

      createOSScene($target, tl)
    }
  }

  /* ======================================================================== */
  /* 22. sectionMasthead */
  /* ======================================================================== */
  var SectionMasthead = function () {
    const $target = $('.section-masthead[data-os-animation]')
    const $heading = $target.find('h1')
    const $meta = $target.find('.post-meta li')
    const $headline = $target.find('.section-masthead__line')
    const splitMeta = splitLines($meta)
    const splitHeading = splitLines($heading)

    prepare()

    function prepare() {
      if (!$target.length) {
        return
      }

      setLines(splitHeading.words)
      setLines(splitMeta.lines)

      TweenMax.set($headline, {
        scaleY: 0,
        transformOrigin: 'top center',
      })
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }

      $target.each(function () {
        const $el = $(this)
        const elMeta = $el.find(splitMeta.lines)
        const elHeading = $el.find(splitHeading.words)
        const $elHeadline = $el.find($headline)
        const tl = new TimelineMax()

        tl.add(animateLines(elHeading)).add(animateLines(elMeta), '-=0.3').to(
          $elHeadline,
          0.6,
          {
            scaleY: 1,
            ease: Expo.easeInOut,
          },
          '-=0.6'
        )

        createOSScene($el, tl)
      })
    }
  }

  /* ======================================================================== */
  /* 23. sectionSteps */
  /* ======================================================================== */
  var SectionSteps = function () {
    const $target = $('.section-steps[data-os-animation] .section-steps__step')
    const $heading = $target.find('.section-steps__content h2')
    const $text = $target.find('.section-steps__content p')
    const $headline = $target.find('.section-steps__headline')
    const $number = $target.find('.section-steps__number')
    const splitDescr = splitLines($text)
    const splitHeading = splitLines($heading)

    prepare()

    function prepare() {
      if (!$target.length) {
        return
      }

      setLines(splitHeading.words)
      setLines(splitDescr.lines)

      TweenMax.set($headline, {
        scaleX: 0,
        transformOrigin: 'left center',
      })

      TweenMax.set($number, {
        autoAlpha: 0,
        y: '30px',
      })
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }

      $target.each(function () {
        const $el = $(this)
        const $elNumber = $el.find($number)
        const $elHeadline = $el.find($headline)
        const elDescr = $el.find(splitDescr.lines)
        const elHeading = $el.find(splitHeading.words)
        const tl = new TimelineMax()

        tl.to($elNumber, 0.6, {
          autoAlpha: 1,
          y: '0px',
          ease: Power4.easeOut,
        })
          .add(animateLines(elHeading), '-=0.4')
          .add(animateLines(elDescr, 1, 0.1), '-=0.4')
          .to(
            $elHeadline,
            0.6,
            {
              scale: 1,
              ease: Power4.easeOut,
            },
            '-=0.8'
          )

        createOSScene($el, tl)
      })
    }
  }

  /* ======================================================================== */
  /* 24. slider */
  /* ======================================================================== */
  function renderSliderCounter(
    sliderMain,
    sliderCounter,
    slideClass,
    elTotal,
    sliderSecondary
  ) {
    if (!sliderMain.slides.length) {
      return
    }

    const numOfSlides = sliderMain.slides.length
    const startSlides = sliderMain.params.slidesPerView

    const counter = new Swiper(sliderCounter, {
      direction: 'vertical',
      simulateTouch: false,
    })

    for (let index = startSlides; index <= numOfSlides; index++) {
      counter.appendSlide(
        '<div class="swiper-slide"><div class="' +
          slideClass +
          '">0' +
          index +
          '</div></div>'
      )
    }

    $(elTotal).html('0' + numOfSlides)

    sliderMain.controller.control = counter
    counter.controller.control = sliderMain

    if (sliderSecondary) {
      sliderSecondary.controller.control = counter
      counter.controller.control = sliderSecondary
    }
  }

  /* ======================================================================== */
  /* 25. sliderFullscreen */
  /* ======================================================================== */
  var SliderFullscreen1 = function () {
    createSlider()

    function createSlider() {
      if (!$('.js-slider-fullscreen').length) {
        return
      }

      const overlapFactor = 0.5
      const sliderImg = new Swiper('.js-slider-fullscreen__slider-img', {
        autoplay: {
          delay: 5000,
        },
        allowTouchMove: false,
        direction: 'vertical',
        speed: 1000,
        pagination: {
          el: '.js-slider-fullscreen__dots',
          type: 'bullets',
          bulletElement: 'div',
          clickable: true,
          bulletClass: 'slider__dot',
          bulletActiveClass: 'slider__dot_active',
        },
        navigation: {
          prevEl: '.js-slider-fullscreen-arrow-left',
          nextEl: '.js-slider-fullscreen-arrow-right',
        },
        mousewheel: {
          eventsTarged: '.page-wrapper',
          sensitivity: 1,
        },
        keyboard: {
          enabled: true,
        },
        watchSlidesProgress: true,
        on: {
          progress() {
            const swiper = this
            for (let i = 0; i < swiper.slides.length; i++) {
              const slideProgress = swiper.slides[i].progress
              const innerOffset = swiper.width * overlapFactor
              const innerTranslate = slideProgress * innerOffset

              TweenMax.set(swiper.slides[i].querySelector('img'), {
                y: innerTranslate + 'px',
                transition: swiper.params.speed + 'ms',
              })
            }
          },
          touchStart() {
            const swiper = this
            for (let i = 0; i < swiper.slides.length; i++) {
              TweenMax.set(swiper.slides[i].querySelector('img'), {
                transition: '',
              })
            }
          },
        },
      })

      const sliderContent = new Swiper(
        '.js-slider-fullscreen__slider-content',
        {
          speed: 1000,
          effect: 'fade',
          fadeEffect: {
            crossFade: true,
          },
          allowTouchMove: false,
          breakpoints: {
            991: {
              autoHeight: true,
            },
          },
        }
      )

      renderSliderCounter(
        sliderImg,
        '.js-slider-fullscreen__counter-current',
        '',
        '.js-slider-fullscreen__counter-total',
        sliderContent
      )
    }
  }

  /* ======================================================================== */
  /* 26. sliderFullscreen4 */
  /* ======================================================================== */
  var SliderFullscreen4 = function () {
    createSlider()

    function createSlider() {
      if (!$('.js-slider-fullscreen4').length) {
        return
      }

      const slider = new Swiper('.js-slider-fullscreen4', {
        slidesPerView: 4,
        speed: 1000,
        autoplay: {
          delay: 5000,
        },
        pagination: {
          el: '.js-slider-fullscreen4-progress',
          type: 'progressbar',
          progressbarFillClass: 'slider__progressbar-fill',
          renderProgressbar(progressbarFillClass) {
            return (
              '<div class="slider__progressbar"><div class="' +
              progressbarFillClass +
              '"></div></div>'
            )
          },
        },
        navigation: {
          prevEl: '.js-slider-fullscreen4-arrow-left',
          nextEl: '.js-slider-fullscreen4-arrow-right',
        },
        mousewheel: {
          eventsTarged: '.page-wrapper',
          sensitivity: 1,
        },
        keyboard: {
          enabled: true,
        },
        breakpoints: {
          1400: {
            slidesPerView: 3,
          },
          991: {
            slidesPerView: 2,
          },
          576: {
            slidesPerView: 1,
          },
        },
      })

      renderSliderCounter(
        slider,
        '.js-slider-fullscreen4-counter-current',
        '',
        '.js-slider-fullscreen4-counter-total'
      )
    }
  }

  /* ======================================================================== */
  /* 27. sliderPortfolioItem */
  /* ======================================================================== */
  var SliderPortfolioItem = function () {
    if (!$('.js-slider-portfolio-item').length) {
      return
    }

    const slider = new Swiper('.js-slider-portfolio-item', {
      autoplay: {
        delay: 6000,
      },
      autoHeight: true,
      speed: 800,
      pagination: {
        el: '.js-slider-portfolio-item-progress',
        type: 'progressbar',
        progressbarFillClass: 'slider__progressbar-fill',
        renderProgressbar(progressbarFillClass) {
          return (
            '<div class="slider__progressbar"><div class="' +
            progressbarFillClass +
            '"></div></div>'
          )
        },
      },
      navigation: {
        prevEl: '.js-slider-portfolio-item__arrow-left',
        nextEl: '.js-slider-portfolio-item__arrow-right',
      },
    })

    renderSliderCounter(
      slider,
      '.js-slider-portfolio-item-counter-current',
      '',
      '.js-slider-portfolio-item-counter-total'
    )
  }

  /* ======================================================================== */
  /* 28. sliderServices */
  /* ======================================================================== */
  var SliderServices = function () {
    const $target = $('.slider-services[data-os-animation]')
    const tl = new TimelineMax()
    const $headline = $target.find('.figure-service__headline')
    const $heading = $target.find('.figure-service__header h3')
    const $counters = $target.find('.figure-service__number')
    const $icons = $target.find('.figure-service__icon')
    const splitHeading = splitLines($heading)

    prepare()
    createSlider()

    function prepare() {
      if (!$target.length) {
        return
      }

      setLines(splitHeading.words)

      TweenMax.set($headline, {
        scaleX: 0,
        transformOrigin: 'center center',
      })

      TweenMax.set([$counters, $icons], {
        y: '30px',
        autoAlpha: 0,
      })
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }

      tl.staggerTo(
        $headline,
        0.6,
        {
          scaleX: 1,
          ease: Expo.easeInOut,
        },
        0.05
      )
        .add(animateLines(splitHeading.words), '-=0.6')
        .staggerTo(
          $counters,
          0.6,
          {
            y: '0px',
            autoAlpha: 1,
          },
          0.1,
          '-=0.6'
        )
        .staggerTo(
          $icons,
          0.6,
          {
            y: '0px',
            autoAlpha: 1,
          },
          0.1,
          '-=0.6'
        )

      createOSScene($target, tl)
    }

    function createSlider() {
      if (!$('.js-slider-services').length) {
        return
      }

      const slider = new Swiper('.js-slider-services', {
        slidesPerView: 4,
        speed: 800,
        autoplay: {
          delay: 5000,
        },
        pagination: {
          el: '.js-slider-services-progress',
          type: 'progressbar',
          progressbarFillClass: 'slider__progressbar-fill',
          renderProgressbar(progressbarFillClass) {
            return (
              '<div class="slider__progressbar"><div class="' +
              progressbarFillClass +
              '"></div></div>'
            )
          },
        },
        navigation: {
          prevEl: '.js-slider-services__arrow-left',
          nextEl: '.js-slider-services__arrow-right',
        },
        breakpoints: {
          1400: {
            slidesPerView: 3,
          },
          991: {
            slidesPerView: 2,
          },
          576: {
            slidesPerView: 1,
          },
        },
      })

      renderSliderCounter(
        slider,
        '.js-slider-services-counter-current',
        '',
        '.js-slider-services-counter-total'
      )
    }
  }

  /* ======================================================================== */
  /* 29. sliderTestimonials */
  /* ======================================================================== */
  var SliderTestimonials = function () {
    const $target = $('.slider-testimonials[data-os-animation]')
    const tl = new TimelineMax()
    const $text = $target.find('.slider-testimonials__text')
    const splitTestimonial = splitLines($text)

    prepare()
    createSlider()

    function prepare() {
      if (!$target.length) {
        return
      }

      setLines(splitTestimonial.lines)
    }

    this.animate = function () {
      if (!$target.length) {
        return
      }

      tl.add(animateLines(splitTestimonial.lines, 1, 0.1))

      createOSScene($target, tl)
    }

    function createSlider() {
      if (!$('.js-slider-testimonials').length) {
        return
      }

      const slider = new Swiper('.js-slider-testimonials', {
        autoHeight: true,
        speed: 800,
        autoplay: {
          delay: 5000,
        },
        navigation: {
          prevEl: '.js-slider-testimonials__arrow-prev',
          nextEl: '.js-slider-testimonials__arrow-next',
        },
        pagination: {
          el: '.js-slider-testimonials__dots',
          type: 'bullets',
          bulletElement: 'div',
          clickable: true,
          bulletClass: 'slider__dot',
          bulletActiveClass: 'slider__dot_active',
        },
      })

      renderSliderCounter(
        slider,
        '.js-slider-testimonials-counter-current',
        'slider-testimonials__counter-current',
        '.js-slider-testimonials-counter-total'
      )
    }
  }

  /* ======================================================================== */
  /* 30. social */
  /* ======================================================================== */
  var Social = function () {
    const $elements = $('.social__item a')

    if (!$elements.length) {
      return
    }

    const circle = new Circle()

    $elements.each(function () {
      circle.animate($(this))
    })
  }

  /* ======================================================================== */
  /* 31. splitText */
  /* ======================================================================== */
  function splitLines($el) {
    if (!$el.length) {
      return false
    }

    return new SplitText($el, {
      type: 'words, lines',
      linesClass: 'split-line',
      wordsClass: 'split-word',
    })
  }

  function setLines(el) {
    if (!$(el).length) {
      return false
    }

    return TweenMax.set(el, {
      y: '150%',
      autoAlpha: 0,
    })
  }

  function animateLines(el, customDuration, customStagger) {
    if (!$(el).length) {
      return false
    }

    const duration = customDuration || 0.6
    const stagger = customStagger || 0.03

    return TweenMax.staggerTo(
      el,
      duration,
      {
        y: '0%',
        autoAlpha: 1,
        ease: Power4.easeOut,
      },
      stagger
    )
  }

  function hideLines(el, customDuration, customStagger) {
    if (!$(el).length) {
      return false
    }

    const duration = customDuration || 0.6
    const stagger = customStagger || 0.03

    return TweenMax.staggerTo(
      el,
      duration,
      {
        y: '150%',
        autoAlpha: 0,
        ease: Power4.easeIn,
      },
      stagger
    )
  }
}
